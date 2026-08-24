use super::{CacheIdentity, CacheKey, CacheShardKey, SourceMessageCache};
use crate::clients::ClientId;
use crate::message_cache::dirs::{
    cache_lock_path, cache_shard_dir, ensure_cache_dir, warn_cache_failure_once,
};
use crate::message_cache::shard::{parse_shard_filename, read_shard, ShardReadStatus};
use std::fs::{self, OpenOptions};

impl SourceMessageCache {
    pub(crate) fn load() -> Self {
        Self::load_identities(CacheIdentity::all())
    }

    /// Load only parser namespaces requested by the current scan.
    ///
    /// Shards are independent, so a Claude/Codex dashboard does not need to
    /// deserialize cache payloads for every legacy parser. The unfiltered
    /// loader remains available for broad callers and migration tests.
    pub(crate) fn load_for_clients(clients: &[String]) -> Self {
        if clients.is_empty() {
            return Self::load();
        }

        let mut identities: Vec<CacheIdentity> = clients
            .iter()
            .filter_map(|client| {
                if client == "synthetic" {
                    Some(CacheIdentity::synthetic())
                } else {
                    ClientId::from_str(client).map(CacheIdentity::for_client)
                }
            })
            .collect();
        identities.sort_unstable_by_key(|identity| identity.namespace);
        identities.dedup();
        Self::load_identities(identities)
    }

    fn load_identities(identities: impl IntoIterator<Item = CacheIdentity>) -> Self {
        let Some(shard_root) = cache_shard_dir() else {
            return Self::default();
        };
        let Some(lock_path) = cache_lock_path() else {
            return Self::default();
        };
        if let Err(error) = ensure_cache_dir(&shard_root) {
            warn_cache_failure_once(
                "source message cache directory is unavailable",
                &shard_root,
                &error,
            );
            return Self::default();
        }
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
                return Self::default();
            }
        };
        if let Err(error) = fs2::FileExt::lock_shared(&lock_file) {
            warn_cache_failure_once("source message cache lock failed", &lock_path, &error);
            return Self::default();
        }

        let mut cache = Self::default();
        for identity in identities {
            let parser_dir = shard_root.join(identity.namespace);
            let read_dir = match fs::read_dir(&parser_dir) {
                Ok(read_dir) => read_dir,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    warn_cache_failure_once(
                        "source message cache parser directory is unreadable",
                        &parser_dir,
                        &error,
                    );
                    continue;
                }
            };

            for dir_entry in read_dir.filter_map(Result::ok) {
                let Some(index) = parse_shard_filename(&dir_entry.file_name()) else {
                    continue;
                };
                let shard_key = CacheShardKey {
                    namespace: identity.namespace.to_string(),
                    index,
                };
                let path = dir_entry.path();
                let (entries, migrated) = match read_shard(&path, identity) {
                    ShardReadStatus::Loaded(entries) => (entries, false),
                    ShardReadStatus::Migrated(entries) => (entries, true),
                    ShardReadStatus::Missing => continue,
                    ShardReadStatus::Stale => {
                        cache.rewrite_shards.insert(shard_key);
                        continue;
                    }
                    ShardReadStatus::Invalid(error) => {
                        warn_cache_failure_once(
                            "source message cache shard is invalid",
                            &path,
                            &error,
                        );
                        cache.rewrite_shards.insert(shard_key);
                        continue;
                    }
                };
                if migrated {
                    cache.rewrite_shards.insert(shard_key.clone());
                }
                for mut entry in entries {
                    let key = CacheKey::from_entry(&entry);
                    if key.shard() == shard_key && entry.identity_is_current() {
                        if entry.parser_namespace == ClientId::Claude.as_str()
                            && crate::sessions::claudecode::remove_synthetic_placeholder_messages(
                                &mut entry.messages,
                            )
                        {
                            // Do not bump Claude's parser version here: compacted
                            // transcripts rely on cached assistant history that a
                            // full invalidation cannot recover. Repair only the bad
                            // `<synthetic>` rows and persist that narrow migration.
                            cache.dirty_keys.insert(key.clone());
                        }
                        cache.entries.insert(key, entry);
                    } else {
                        cache.rewrite_shards.insert(shard_key.clone());
                    }
                }
            }
        }

        cache.dirty = !(cache.rewrite_shards.is_empty() && cache.dirty_keys.is_empty());
        cache
    }
}
