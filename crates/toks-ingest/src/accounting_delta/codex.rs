use crate::sessions::codex::{parse_codex_file_range, CodexParseState};

use super::fingerprint::{
    complete_codex_boundary, hash_range, metadata, prefix_samples, revision, samples_match,
    CODEX_CHUNK_BYTES,
};
use super::store::StoredCheckpoint;
use super::types::{
    CollectContext, ProcessedSource, SourceCandidate, SourceCheckpoint, SourceDelta, SourceKind,
};

pub(crate) fn parser_version() -> u32 {
    crate::message_cache::parser_version(crate::ClientId::Codex)
}

pub(crate) fn process(
    source: &SourceCandidate,
    previous: Option<&StoredCheckpoint>,
    seed: Option<crate::message_cache::CodexAccountingSeed>,
    context: &CollectContext<'_>,
) -> Result<ProcessedSource, String> {
    let current_parser = parser_version();
    let legacy_checkpoint = previous.is_some_and(|checkpoint| {
        checkpoint.kind == SourceKind::Codex
            && checkpoint.parser_version.saturating_add(1) == current_parser
            && checkpoint.committed_offset < source.size
            && samples_match(&source.path, &checkpoint.prefix_samples)
    });
    let legacy_identity_state =
        legacy_checkpoint || seed.as_ref().is_some_and(|seed| seed.legacy_identity_state);
    let can_append = previous.is_some_and(|checkpoint| {
        checkpoint.kind == SourceKind::Codex
            && (checkpoint.parser_version == current_parser || legacy_checkpoint)
            && checkpoint.committed_offset <= source.size
            && samples_match(&source.path, &checkpoint.prefix_samples)
    });
    let (start, state) = if can_append {
        let checkpoint = previous.expect("checked above");
        (
            checkpoint.committed_offset,
            checkpoint.codex_state.clone().unwrap_or_default(),
        )
    } else if let Some(seed) = seed.as_ref() {
        (seed.consumed_offset, seed.state.clone())
    } else {
        (0, CodexParseState::default())
    };
    let chunk = parse_semantic_chunk(source, start, state)?;
    if chunk.is_none() && seed.is_none() {
        return Ok(ProcessedSource {
            delta: None,
            // An incomplete live record is retried on the normal poll, not an
            // immediate CatchingUp loop.
            remains_pending: false,
        });
    }
    let mut observations = seed
        .as_ref()
        .map(|seed| seed.messages.clone())
        .unwrap_or_default();
    let mut fallback_indices = seed
        .as_ref()
        .map(|seed| seed.fallback_timestamp_indices.clone())
        .unwrap_or_default();
    let (boundary, checkpoint_state, content_hash) = if let Some(chunk) = chunk {
        let base = observations.len();
        observations.extend(chunk.parsed.messages);
        fallback_indices.extend(
            chunk
                .parsed
                .fallback_timestamp_indices
                .into_iter()
                .map(|index| base + index),
        );
        (chunk.boundary, chunk.parsed.state, chunk.content_hash)
    } else {
        let seed = seed.as_ref().expect("checked above");
        (seed.consumed_offset, seed.state.clone(), seed.prefix_hash)
    };
    if seed.is_some() {
        tracing::debug!(
            messages = observations.len(),
            "seeded accounting frontier from the source message cache"
        );
    }
    let after = metadata(&source.path)?;
    let fallback_timestamp = crate::sessions::utils::file_modified_timestamp_ms(&source.path);
    for index in fallback_indices {
        if let Some(message) = observations.get_mut(index) {
            message.set_timestamp(fallback_timestamp);
        }
    }
    for message in &mut observations {
        message.refresh_derived_fields();
        crate::apply_pricing_if_available(message, context.pricing);
        crate::apply_headless_agent(message, true);
        if legacy_identity_state {
            // A v6 frontier predates the occurrence tracker. Keep these
            // observations source-scoped/weak until the forced current-parser
            // replay upgrades them; never mint unstable typed identities.
            message.durable_identity = None;
        }
    }
    let mut checkpoint_state = checkpoint_state;
    // Workspace keys are raw local paths. They are presentation metadata, not
    // accounting state, so never put them in the durable checkpoint.
    checkpoint_state.session_workspace_key = None;
    checkpoint_state.session_workspace_label = None;
    let parser_version = current_parser;
    let checkpoint_parser_version = if legacy_identity_state {
        parser_version.saturating_sub(1)
    } else {
        parser_version
    };
    let proposed = StoredCheckpoint {
        kind: SourceKind::Codex,
        parser_version: checkpoint_parser_version,
        committed_offset: boundary,
        source_size: after.size,
        modified_ns: after.modified_ns,
        content_hash,
        prefix_samples: prefix_samples(&source.path, boundary)?,
        codex_state: Some(checkpoint_state),
    };
    let previous_offset = previous.map_or(0, |checkpoint| checkpoint.committed_offset);
    let offset_bytes = start.to_le_bytes();
    let boundary_bytes = boundary.to_le_bytes();
    let parser_bytes = parser_version.to_le_bytes();
    let source_revision = revision(&[
        source.key.as_str().as_bytes(),
        &parser_bytes,
        &offset_bytes,
        &boundary_bytes,
        &content_hash,
    ]);
    let source_complete = complete_codex_boundary(&source.path, boundary, after.size, 1)?.is_none();
    let complete = source_complete && !legacy_identity_state;
    Ok(ProcessedSource {
        delta: Some(SourceDelta {
            source_key: source.key.clone(),
            revision: source_revision,
            observations,
            checkpoint: SourceCheckpoint {
                parser_version: checkpoint_parser_version,
                previous_offset,
                committed_offset: boundary,
                source_size: after.size,
            },
            backfill_complete: complete,
            proposed,
        }),
        remains_pending: !complete,
    })
}

struct SemanticChunk {
    boundary: u64,
    parsed: crate::sessions::codex::ParsedCodexFile,
    content_hash: [u8; 32],
}

fn parse_semantic_chunk(
    source: &SourceCandidate,
    start: u64,
    state: CodexParseState,
) -> Result<Option<SemanticChunk>, String> {
    let Some(mut boundary) =
        complete_codex_boundary(&source.path, start, source.size, CODEX_CHUNK_BYTES)?
    else {
        return Ok(None);
    };
    let semantic_limit = boundary.saturating_add(CODEX_CHUNK_BYTES);
    loop {
        let before = hash_range(&source.path, start, boundary)?;
        let parsed = parse_codex_file_range(&source.path, start, boundary, state.clone());
        let after = hash_range(&source.path, start, boundary)?;
        if before != after || parsed.consumed_offset != boundary {
            return Ok(None);
        }
        if parsed.unresolved_model_events && boundary < semantic_limit {
            if let Some(next) = complete_codex_boundary(&source.path, boundary, source.size, 1)? {
                boundary = next;
                continue;
            }
        }
        if !parsed.parse_succeeded {
            tracing::warn!("malformed Codex JSONL record skipped during accounting ingest");
        }
        if parsed.unresolved_model_events {
            tracing::warn!("Codex usage without model context was retained as unknown");
        }
        return Ok(Some(SemanticChunk {
            boundary,
            parsed,
            content_hash: after,
        }));
    }
}
