use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::path::{Path, PathBuf};

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

/// Load only the shard that can contain `path`; never deserialize every Codex
/// shard just to seed one accounting frontier.
#[cfg(test)]
pub(crate) fn load_codex_accounting_seed(path: &Path) -> Option<CodexAccountingSeed> {
    load_codex_accounting_seeds(std::iter::once(path)).remove(path)
}

/// Load one bounded collector batch while opening each involved shard once.
pub(crate) fn load_codex_accounting_seeds<'a>(
    paths: impl Iterator<Item = &'a Path>,
) -> HashMap<PathBuf, CodexAccountingSeed> {
    let identity = CacheIdentity::for_client(ClientId::Codex);
    let mut requests = HashMap::new();
    for path in paths {
        let key = CacheKey::new(identity, path);
        requests
            .entry(key.shard())
            .or_insert_with(Vec::new)
            .push((path.to_path_buf(), key));
    }
    if requests.is_empty() {
        return HashMap::new();
    }
    let Some(shard_root) = cache_shard_dir() else {
        return HashMap::new();
    };
    if ensure_cache_dir(&shard_root).is_err() {
        return HashMap::new();
    }
    let Some(lock_path) = cache_lock_path() else {
        return HashMap::new();
    };
    let Ok(lock) = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
    else {
        return HashMap::new();
    };
    if fs2::FileExt::try_lock_shared(&lock).is_err() {
        return HashMap::new();
    }
    let mut seeds = HashMap::new();
    for (shard_key, requested) in requests {
        #[cfg(test)]
        SHARD_READS.with(|reads| reads.set(reads.get() + 1));
        let Some((entries, legacy_identity_state)) =
            read_seed_shard(&shard_path(&shard_root, &shard_key), identity)
        else {
            continue;
        };
        let expected_parser = identity
            .parser_version
            .saturating_sub(u32::from(legacy_identity_state));
        let mut by_key: HashMap<_, _> = entries
            .into_iter()
            .filter(|entry| {
                entry.parser_namespace == identity.namespace
                    && entry.parser_version == expected_parser
            })
            .map(|entry| (CacheKey::from_entry(&entry), entry))
            .collect();
        for (path, key) in requested {
            let Some(entry) = by_key.remove(&key) else {
                continue;
            };
            let Some(incremental) = entry.codex_incremental.as_ref() else {
                continue;
            };
            if !codex_prefix_matches(&path, incremental) {
                continue;
            }
            seeds.insert(
                path,
                CodexAccountingSeed {
                    messages: entry.messages,
                    fallback_timestamp_indices: entry.fallback_timestamp_indices,
                    state: incremental.state.clone(),
                    consumed_offset: incremental.consumed_offset,
                    prefix_hash: incremental.prefix_hash,
                    legacy_identity_state,
                },
            );
        }
    }
    seeds
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
