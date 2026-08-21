use crate::sessions::codex::CodexParseState;

use super::fingerprint::{
    complete_codex_boundary, metadata, prefix_samples, revision, samples_match,
};
use super::store::StoredCheckpoint;
#[cfg(test)]
use super::types::SourceCheckpoint;
use super::types::{CollectContext, ProcessedSource, SourceCandidate, SourceDelta, SourceKind};

mod chunk;
mod version;

pub(crate) fn parser_version() -> u32 {
    version::current()
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
            && checkpoint.parser_version == version::LEGACY_IDENTITY
            && checkpoint.committed_offset < source.size
            && samples_match(&source.path, &checkpoint.prefix_samples)
    });
    let legacy_identity_state =
        legacy_checkpoint || seed.as_ref().is_some_and(|seed| seed.legacy_identity_state);
    let can_append = previous.is_some_and(|checkpoint| {
        checkpoint.kind == SourceKind::Codex
            && (checkpoint.parser_version == current_parser
                || checkpoint.parser_version == version::JSON_WIRE_COMPATIBLE
                || legacy_checkpoint)
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
    let chunk = chunk::parse(source, start, state)?;
    if chunk.is_none() && seed.is_none() {
        return Ok(ProcessedSource {
            delta: None,
            // An incomplete live record is retried on the normal poll, not an
            // immediate CatchingUp loop.
            remains_pending: false,
        });
    }
    let mut seed = seed;
    let mut observations = seed
        .as_mut()
        .map(|seed| std::mem::take(&mut seed.messages))
        .unwrap_or_default();
    let mut fallback_indices = seed
        .as_mut()
        .map(|seed| std::mem::take(&mut seed.fallback_timestamp_indices))
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
            // Keep pre-tracker observations weak until the current-parser
            // replay upgrades them; never mint unstable typed identities.
            message.durable_identity = None;
        }
    }
    let mut checkpoint_state = checkpoint_state;
    // Workspace paths are presentation metadata, not accounting state.
    checkpoint_state.session_workspace_key = None;
    checkpoint_state.session_workspace_label = None;
    let parser_version = current_parser;
    let checkpoint_parser_version = if legacy_identity_state {
        version::LEGACY_IDENTITY
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
    #[cfg(test)]
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
            #[cfg(test)]
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

pub(crate) fn checkpoint_version_is_current(version: u32) -> bool {
    version::is_current(version)
}
