use super::{
    CacheIdentity, CacheKey, CacheShardKey, CachedSourceEntry, DeletionReason, SourceMessageCache,
};
use crate::clients::ClientId;
use crate::message_cache::dirs::{
    cache_lock_path, cache_shard_dir, ensure_cache_dir, warn_cache_failure_once,
};
use crate::message_cache::shard::{
    read_shard_with_limit, shard_path, write_shard_with_limit, ShardReadStatus,
    MAX_CACHE_SHARD_BYTES,
};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;

impl SourceMessageCache {
    pub(crate) fn save_if_dirty(&mut self) {
        self.save_if_dirty_with_limit(MAX_CACHE_SHARD_BYTES);
    }

    pub(in crate::message_cache) fn save_if_dirty_with_limit(&mut self, max_shard_bytes: u64) {
        if !self.dirty {
            return;
        }

        let Some(shard_root) = cache_shard_dir() else {
            return;
        };
        if let Err(error) = ensure_cache_dir(&shard_root) {
            warn_cache_failure_once(
                "source message cache directory is unavailable",
                &shard_root,
                &error,
            );
            return;
        }
        let Some(lock_path) = cache_lock_path() else {
            return;
        };
        let lock_file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) => {
                warn_cache_failure_once(
                    "source message cache lock is unavailable",
                    &lock_path,
                    &error,
                );
                return;
            }
        };
        if let Err(error) = fs2::FileExt::lock_exclusive(&lock_file) {
            warn_cache_failure_once("source message cache lock failed", &lock_path, &error);
            return;
        }

        // Bucket dirty and deleted keys by shard up front. CacheKey::shard()
        // computes a SHA-256 digest, so grouping once keeps hashing at O(keys).
        // The previous per-shard `.filter(|k| k.shard() == shard_key)` recomputed
        // that digest for every key on every shard — O(shards * keys) — which
        // dominated cold-cache builds (hundreds of shards * tens of thousands of
        // files re-hashed).
        let mut dirty_by_shard: HashMap<CacheShardKey, Vec<CacheKey>> = HashMap::new();
        for key in &self.dirty_keys {
            dirty_by_shard
                .entry(key.shard())
                .or_default()
                .push(key.clone());
        }
        let mut deleted_by_shard: HashMap<CacheShardKey, Vec<(CacheKey, DeletionReason)>> =
            HashMap::new();
        for (key, reason) in &self.deleted_keys {
            deleted_by_shard
                .entry(key.shard())
                .or_default()
                .push((key.clone(), reason.clone()));
        }

        let mut affected_shards = self.rewrite_shards.clone();
        affected_shards.extend(dirty_by_shard.keys().cloned());
        affected_shards.extend(deleted_by_shard.keys().cloned());

        let mut successful_shards = HashSet::new();
        for shard_key in affected_shards {
            let Some(identity) = CacheIdentity::current_for_namespace(&shard_key.namespace) else {
                continue;
            };
            let parser_dir = shard_root.join(identity.namespace);
            if let Err(error) = ensure_cache_dir(&parser_dir) {
                warn_cache_failure_once(
                    "source message cache parser directory is unavailable",
                    &parser_dir,
                    &error,
                );
                continue;
            }
            let final_path = shard_path(&shard_root, &shard_key);

            let mut merged_entries: HashMap<CacheKey, CachedSourceEntry> =
                match read_shard_with_limit(&final_path, identity, max_shard_bytes) {
                    ShardReadStatus::Loaded(entries) | ShardReadStatus::Migrated(entries) => {
                        entries
                            .into_iter()
                            .filter(|entry| entry.identity_is_current())
                            .map(|entry| (CacheKey::from_entry(&entry), entry))
                            .filter(|(key, _)| key.shard() == shard_key)
                            .collect()
                    }
                    ShardReadStatus::Missing | ShardReadStatus::Stale => HashMap::new(),
                    ShardReadStatus::Invalid(error) => {
                        warn_cache_failure_once(
                            "source message cache shard is invalid",
                            &final_path,
                            &error,
                        );
                        HashMap::new()
                    }
                };

            if let Some(deleted) = deleted_by_shard.get(&shard_key) {
                for (key, reason) in deleted {
                    let should_remove = match reason {
                        DeletionReason::Missing => !key.path.to_path_buf().exists(),
                        DeletionReason::Invalidated(expected) => merged_entries
                            .get(key)
                            .is_some_and(|entry| entry.fingerprint == *expected),
                    };
                    if should_remove {
                        merged_entries.remove(key);
                    }
                }
            }
            if let Some(dirty) = dirty_by_shard.get(&shard_key) {
                for key in dirty {
                    if let Some(entry) = self.entries.get(key) {
                        let mut entry = entry.clone();
                        // Another process holding the lock before us may have
                        // stored history for this same path that our in-memory
                        // entry never saw. Union it in rather than replacing
                        // wholesale — see `absorb_retained_history`.
                        if let Some(stored) = merged_entries.remove(key) {
                            entry.absorb_retained_history(&stored);
                        }
                        if entry.parser_namespace == ClientId::Claude.as_str() {
                            crate::sessions::claudecode::remove_synthetic_placeholder_messages(
                                &mut entry.messages,
                            );
                        }
                        merged_entries.insert(key.clone(), entry);
                    }
                }
            }

            let mut entries: Vec<CachedSourceEntry> = merged_entries.into_values().collect();
            entries.sort_by_key(|left| left.path.to_path_buf());
            match write_shard_with_limit(&final_path, identity, &entries, max_shard_bytes) {
                Ok(()) => {
                    successful_shards.insert(shard_key);
                }
                Err(error) => {
                    warn_cache_failure_once(
                        "source message cache shard could not be saved; future scans may remain cold",
                        &final_path,
                        &error,
                    );
                }
            }
        }

        self.dirty_keys
            .retain(|key| !successful_shards.contains(&key.shard()));
        self.deleted_keys
            .retain(|key, _| !successful_shards.contains(&key.shard()));
        self.rewrite_shards
            .retain(|shard| !successful_shards.contains(shard));
        self.dirty = !(self.dirty_keys.is_empty()
            && self.deleted_keys.is_empty()
            && self.rewrite_shards.is_empty());
    }
}
