use super::CachedPath;
use serde::{Deserialize, Serialize};

mod build;
mod check;
mod check_clients;
mod check_related;
mod codex;
mod related;

#[cfg(test)]
pub(crate) use codex::build_codex_incremental_cache;
pub(crate) use codex::{
    build_codex_incremental_cache_with_prefix_hash, codex_cache_entry_matches_fingerprint,
    codex_prefix_matches, CodexIncrementalCache,
};
#[cfg(test)]
pub(super) use related::metadata_signature;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct FileSampleHash {
    pub offset: u64,
    pub len: u64,
    pub hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceFingerprint {
    pub size: u64,
    pub modified_ns: u64,
    pub sample_hashes: Vec<FileSampleHash>,
    pub content_hash: [u8; 32],
    pub related_files: Vec<RelatedFileFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct RelatedFileFingerprint {
    pub suffix: String,
    pub path: CachedPath,
    pub exists: bool,
    pub size: u64,
    pub modified_ns: u64,
    pub sample_hashes: Vec<FileSampleHash>,
    pub content_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FingerprintStatus {
    /// Size and nanosecond mtime still match for the source and every parser
    /// sidecar, and their bounded samples still match. No full-file SHA-256 was
    /// computed, so a warm scan reads at most 20 KiB per watched file.
    Unchanged,
    /// Metadata changed, so a complete fingerprint was rebuilt to distinguish
    /// a real content change from a metadata-only touch.
    Changed(SourceFingerprint),
}
