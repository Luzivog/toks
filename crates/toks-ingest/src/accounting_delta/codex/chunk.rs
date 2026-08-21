use crate::sessions::codex::{parse_codex_file_range, CodexParseState, ParsedCodexFile};

use crate::accounting_delta::fingerprint::{
    complete_codex_boundary, hash_range, CODEX_CHUNK_BYTES,
};
use crate::accounting_delta::types::SourceCandidate;

pub(super) struct SemanticChunk {
    pub boundary: u64,
    pub parsed: ParsedCodexFile,
    pub content_hash: [u8; 32],
}

pub(super) fn parse(
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
