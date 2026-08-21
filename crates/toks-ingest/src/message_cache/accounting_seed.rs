use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::path::Path;

use bincode::Options;

use crate::sessions::codex::CodexParseState;
use crate::{ClientId, UnifiedMessage};

use super::{
    cache_lock_path, cache_shard_dir, codex_prefix_matches, ensure_cache_dir, shard_path,
    CacheIdentity, CacheKey, CachedShardEnvelope, CachedSourceEntry, CACHE_FORMAT_VERSION,
    MAX_CACHE_SHARD_BYTES,
};

pub(crate) struct CodexAccountingSeed {
    pub messages: Vec<UnifiedMessage>,
    pub fallback_timestamp_indices: Vec<usize>,
    pub state: CodexParseState,
    pub consumed_offset: u64,
    pub prefix_hash: [u8; 32],
    pub legacy_identity_state: bool,
}

#[cfg(test)]
thread_local! {
    static SHARD_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn load_codex_accounting_seed(path: &Path) -> Option<CodexAccountingSeed> {
    let identity = CacheIdentity::for_client(ClientId::Codex);
    let key = CacheKey::new(identity, path);
    let shard_root = cache_shard_dir()?;
    if ensure_cache_dir(&shard_root).is_err() {
        return None;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache_lock_path()?)
        .ok()?;
    if fs2::FileExt::try_lock_shared(&lock).is_err() {
        return None;
    }
    #[cfg(test)]
    SHARD_READS.with(|reads| reads.set(reads.get() + 1));
    let (entries, legacy_identity_state) =
        read_seed_shard(&shard_path(&shard_root, &key.shard()), identity)?;
    let expected_parser = identity
        .parser_version
        .saturating_sub(u32::from(legacy_identity_state));
    let entry = entries.into_iter().find(|entry| {
        entry.parser_namespace == identity.namespace
            && entry.parser_version == expected_parser
            && CacheKey::from_entry(entry) == key
    })?;
    let incremental = entry.codex_incremental.as_ref()?;
    if !codex_prefix_matches(path, incremental) {
        return None;
    }
    Some(CodexAccountingSeed {
        messages: entry.messages,
        fallback_timestamp_indices: entry.fallback_timestamp_indices,
        state: incremental.state.clone(),
        consumed_offset: incremental.consumed_offset,
        prefix_hash: incremental.prefix_hash,
        legacy_identity_state,
    })
}

fn read_seed_shard(path: &Path, identity: CacheIdentity) -> Option<(Vec<CachedSourceEntry>, bool)> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_CACHE_SHARD_BYTES {
        return None;
    }
    let envelope: CachedShardEnvelope = bincode::options()
        .with_limit(MAX_CACHE_SHARD_BYTES)
        .deserialize_from(BufReader::new(file))
        .ok()?;
    if envelope.parser_namespace != identity.namespace {
        return None;
    }
    let legacy_identity_state =
        envelope.parser_version.saturating_add(1) == identity.parser_version;
    if envelope.parser_version != identity.parser_version && !legacy_identity_state {
        return None;
    }
    let entries = if let Some(decoded) = super::legacy_wire::decode(
        envelope.format_version,
        &envelope.payload,
        MAX_CACHE_SHARD_BYTES,
    ) {
        decoded.ok()?
    } else if !legacy_identity_state && envelope.format_version == CACHE_FORMAT_VERSION {
        bincode::options()
            .with_limit(MAX_CACHE_SHARD_BYTES)
            .deserialize(&envelope.payload)
            .ok()?
    } else {
        return None;
    };
    Some((entries, legacy_identity_state))
}

#[cfg(test)]
pub(crate) fn reset_shard_read_count() {
    SHARD_READS.with(|reads| reads.set(0));
}

#[cfg(test)]
pub(crate) fn shard_read_count() -> usize {
    SHARD_READS.with(std::cell::Cell::get)
}
