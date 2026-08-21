use super::fingerprint::{hash_range, metadata, prefix_samples, revision};
use super::store::StoredCheckpoint;
#[cfg(test)]
use super::types::SourceCheckpoint;
use super::types::{CollectContext, ProcessedSource, SourceCandidate, SourceDelta, SourceKind};

pub(crate) fn parser_version() -> u32 {
    crate::message_cache::parser_version(crate::ClientId::Claude)
}

pub(crate) fn process(
    source: &SourceCandidate,
    _previous: Option<&StoredCheckpoint>,
    context: &CollectContext<'_>,
) -> Result<ProcessedSource, String> {
    let before = metadata(&source.path)?;
    let mut observations = crate::sessions::claudecode::parse_claude_file_with_home(
        &source.path,
        Some(context.home_dir),
    );
    let after = metadata(&source.path)?;
    if before != after {
        return Ok(ProcessedSource {
            delta: None,
            remains_pending: true,
        });
    }
    let content_hash = hash_range(&source.path, 0, after.size)?;
    for message in &mut observations {
        message.refresh_derived_fields();
        crate::apply_pricing_if_available(message, context.pricing);
    }
    let proposed = StoredCheckpoint {
        kind: SourceKind::Claude,
        parser_version: parser_version(),
        committed_offset: after.size,
        source_size: after.size,
        modified_ns: after.modified_ns,
        content_hash,
        prefix_samples: prefix_samples(&source.path, after.size)?,
        codex_state: None,
    };
    let parser_version = parser_version();
    let parser_bytes = parser_version.to_le_bytes();
    let source_revision = revision(&[source.key.as_str().as_bytes(), &parser_bytes, &content_hash]);
    Ok(ProcessedSource {
        delta: Some(SourceDelta {
            source_key: source.key.clone(),
            revision: source_revision,
            observations,
            #[cfg(test)]
            checkpoint: SourceCheckpoint {
                parser_version,
                previous_offset: _previous.map_or(0, |checkpoint| checkpoint.committed_offset),
                committed_offset: after.size,
                source_size: after.size,
            },
            backfill_complete: true,
            proposed,
        }),
        remains_pending: false,
    })
}
