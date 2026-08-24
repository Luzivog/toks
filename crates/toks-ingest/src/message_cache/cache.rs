use super::{CachedPath, CodexIncrementalCache, SourceFingerprint};
use crate::UnifiedMessage;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod entry;
mod identity;
mod key;
mod load;
mod mutation;
mod save;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CacheIdentity {
    pub(super) namespace: &'static str,
    pub(super) parser_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    namespace: String,
    path: CachedPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct CacheShardKey {
    pub(super) namespace: String,
    pub(super) index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedSourceEntry {
    pub(super) parser_namespace: String,
    pub(super) parser_version: u32,
    pub path: CachedPath,
    pub fingerprint: SourceFingerprint,
    /// Not always a pure function of the source file. For a namespace that
    /// `retained_history_key_filter` covers, this can hold messages the live
    /// file no longer contains, and re-parsing will not reproduce them — the
    /// cache is the only copy. That is what makes a parser_version bump for
    /// those namespaces lossy rather than merely cold.
    pub messages: Vec<UnifiedMessage>,
    pub fallback_timestamp_indices: Vec<usize>,
    pub codex_incremental: Option<CodexIncrementalCache>,
    /// Prime-only metadata used to reconcile fork aggregates with child
    /// transcripts. It shares this entry's parser identity and fingerprint, so
    /// a message cache hit can never pair with accounting from different bytes.
    pub prime_accounting: Option<crate::sessions::prime_agent::PrimeFileAccounting>,
}

#[derive(Debug, Clone)]
enum DeletionReason {
    Invalidated(SourceFingerprint),
    Missing,
}

#[derive(Default)]
pub(crate) struct SourceMessageCache {
    pub entries: HashMap<CacheKey, CachedSourceEntry>,
    pub(super) dirty: bool,
    dirty_keys: HashSet<CacheKey>,
    deleted_keys: HashMap<CacheKey, DeletionReason>,
    pub(super) rewrite_shards: HashSet<CacheShardKey>,
}
