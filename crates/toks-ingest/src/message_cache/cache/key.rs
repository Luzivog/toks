use super::{CacheIdentity, CacheKey, CacheShardKey, CachedSourceEntry};
use crate::message_cache::shard::CACHE_SHARD_COUNT;
use crate::message_cache::CachedPath;
use sha2::{Digest, Sha256};
use std::path::Path;

impl CacheKey {
    pub(in crate::message_cache) fn new(identity: CacheIdentity, path: &Path) -> Self {
        Self {
            namespace: identity.namespace.to_string(),
            path: CachedPath::from_path(path),
        }
    }

    pub(in crate::message_cache) fn from_entry(entry: &CachedSourceEntry) -> Self {
        Self {
            namespace: entry.parser_namespace.clone(),
            path: entry.path.clone(),
        }
    }

    pub(in crate::message_cache) fn shard(&self) -> CacheShardKey {
        let mut hasher = Sha256::new();
        hasher.update(self.namespace.as_bytes());
        hasher.update([0]);
        self.path.update_digest(&mut hasher);
        let digest = hasher.finalize();
        CacheShardKey {
            namespace: self.namespace.clone(),
            index: usize::from(digest[0]) % CACHE_SHARD_COUNT,
        }
    }
}
