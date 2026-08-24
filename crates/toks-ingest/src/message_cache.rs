mod accounting_seed;
mod cache;
mod cached_path;
mod dirs;
mod fingerprint;
mod legacy_codex;
mod legacy_wire;
mod parser_version;
mod shard;

#[cfg(test)]
use legacy_wire::{LegacyCachedSourceEntryV4, LegacyUnifiedMessageV5, FORMAT_V4};
#[cfg(test)]
mod legacy_wire_tests;
pub(crate) use accounting_seed::{load_codex_accounting_seed, CodexAccountingSeed};
#[cfg(test)]
pub(crate) use accounting_seed::{reset_shard_read_count, shard_read_count};
use cache::CacheShardKey;
pub(crate) use cache::{CacheIdentity, CacheKey, CachedSourceEntry, SourceMessageCache};
pub(crate) use cached_path::CachedPath;
use dirs::{cache_lock_path, cache_shard_dir, ensure_cache_dir};
#[cfg(test)]
use dirs::{fallback_cache_dir, warn_cache_failure_once, warn_cache_failure_once_in};
#[cfg(test)]
pub(crate) use fingerprint::build_codex_incremental_cache;
#[cfg(test)]
use fingerprint::metadata_signature;
pub(crate) use fingerprint::{
    build_codex_incremental_cache_with_prefix_hash, codex_cache_entry_matches_fingerprint,
    codex_prefix_matches, CodexIncrementalCache, FileSampleHash, FingerprintStatus,
    RelatedFileFingerprint, SourceFingerprint,
};
pub(crate) use parser_version::parser_version;
#[cfg(test)]
use shard::{append_path_suffix, full_hash_call_count};
#[cfg(test)]
use shard::{read_shard, write_shard_with_limit, ShardReadStatus, CACHE_SHARD_COUNT};
use shard::{shard_path, CachedShardEnvelope, CACHE_FORMAT_VERSION, MAX_CACHE_SHARD_BYTES};

#[cfg(test)]
mod tests;
