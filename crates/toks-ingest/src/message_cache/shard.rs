use crate::message_cache::CachedSourceEntry;
use serde::{Deserialize, Serialize};

mod hashing;
mod io;

#[cfg(test)]
pub(super) use hashing::full_hash_call_count;
pub(super) use hashing::{
    append_path_suffix, compute_sample_hashes, file_fingerprint_parts, hash_prefix, ContentHashMode,
};
pub(super) use io::{
    parse_shard_filename, read_shard, read_shard_with_limit, shard_path, write_shard_with_limit,
};

// CACHE_FORMAT_VERSION changes only when the serialized storage layout or a
// cross-client type such as UnifiedMessage changes incompatibly. Parser-only
// changes belong in parser_version() so one client cannot evict every other
// client's cached transcripts.
// 2: Related-file fingerprints now retain their paths and whether they were
// absent when cached. Claude sidechain parent candidates can therefore be
// revalidated without reparsing the sidechain on every warm scan, while a
// later-created parent transcript still invalidates the entry.
// 3: UnifiedMessage gained session_title, changing the bincode payload layout.
// Old shards must read as Stale (silent rebuild), not Invalid (corruption
// warning), so the format version moves with the struct.
// 4: UnifiedMessage gained model_attribution_conflicted, changing the bincode
// payload layout. Old shards must be silently rebuilt rather than decoded.
// 5: Prime Agent entries cache reconciliation accounting beside their messages.
// Version-4 shards have an explicit wire migration below, so other clients stay
// warm and Prime entries need only one rebuild/backfill.
// 6: UnifiedMessage gained a durable accounting identity and best-effort
// accounting aliases. Explicit v4/v5 wire migrations preserve retained Claude
// turns that a cold reparse cannot recover.
pub(super) const CACHE_FORMAT_VERSION: u32 = 6;
pub(super) const CACHE_SHARD_COUNT: usize = 256;
pub(super) const MAX_CACHE_SHARD_BYTES: u64 = 256 * 1024 * 1024;

/// The envelope is deliberately independent from CachedSourceEntry's binary
/// layout. A parser version can therefore be checked before its payload is
/// deserialized, so (for example) a CodexParseState layout change cannot make
/// Claude's independently sharded cache unreadable.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CachedShardEnvelope {
    pub(super) format_version: u32,
    pub(super) parser_namespace: String,
    pub(super) parser_version: u32,
    pub(super) payload: Vec<u8>,
}

pub(super) enum ShardReadStatus {
    Missing,
    Stale,
    Invalid(String),
    Loaded(Vec<CachedSourceEntry>),
    Migrated(Vec<CachedSourceEntry>),
}
