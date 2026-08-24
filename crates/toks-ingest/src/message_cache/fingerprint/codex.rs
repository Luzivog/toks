use super::SourceFingerprint;
use crate::message_cache::shard::hash_prefix;
use crate::message_cache::CachedSourceEntry;
use crate::sessions::codex::CodexParseState;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CodexIncrementalCache {
    pub state: CodexParseState,
    pub consumed_offset: u64,
    pub ends_with_newline: bool,
    pub prefix_hash: [u8; 32],
}

#[cfg(test)]
pub(crate) fn build_codex_incremental_cache(
    path: &Path,
    consumed_offset: u64,
    state: CodexParseState,
) -> Option<CodexIncrementalCache> {
    let ends_with_newline = consumed_offset == 0 || file_ends_with_newline(path, consumed_offset);
    if !ends_with_newline {
        return None;
    }

    Some(CodexIncrementalCache {
        state,
        consumed_offset,
        ends_with_newline,
        prefix_hash: hash_prefix(path, consumed_offset)?,
    })
}

/// Build Codex incremental state when the caller already hashed the complete
/// consumed prefix. Full-file Codex fingerprints are also the prefix hash when
/// `consumed_offset` equals the current file size, so accepting that digest
/// avoids a second read of the transcript.
pub(crate) fn build_codex_incremental_cache_with_prefix_hash(
    path: &Path,
    consumed_offset: u64,
    state: CodexParseState,
    prefix_hash: [u8; 32],
) -> Option<CodexIncrementalCache> {
    let ends_with_newline = consumed_offset == 0 || file_ends_with_newline(path, consumed_offset);
    if !ends_with_newline {
        return None;
    }

    Some(CodexIncrementalCache {
        state,
        consumed_offset,
        ends_with_newline,
        prefix_hash,
    })
}

fn file_ends_with_newline(path: &Path, size: u64) -> bool {
    if size == 0 {
        return true;
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if file.seek(SeekFrom::Start(size.saturating_sub(1))).is_err() {
        return false;
    }

    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte).is_ok() && byte[0] == b'\n'
}

pub(crate) fn codex_prefix_matches(path: &Path, cached: &CodexIncrementalCache) -> bool {
    if cached.consumed_offset > 0 && !cached.ends_with_newline {
        return false;
    }

    match hash_prefix(path, cached.consumed_offset) {
        Some(prefix_hash) => prefix_hash == cached.prefix_hash,
        None => false,
    }
}

pub(crate) fn codex_cache_entry_matches_fingerprint(
    cached: &CachedSourceEntry,
    fingerprint: &SourceFingerprint,
) -> bool {
    let Some(codex_incremental) = cached.codex_incremental.as_ref() else {
        return false;
    };

    codex_incremental.consumed_offset == fingerprint.size
        && codex_incremental.ends_with_newline
        && codex_incremental.prefix_hash == fingerprint.content_hash
}
