use super::{CacheIdentity, CacheKey, CachedSourceEntry, DeletionReason, SourceMessageCache};
use std::path::Path;

impl SourceMessageCache {
    pub(crate) fn insert(&mut self, entry: CachedSourceEntry) {
        let key = CacheKey::from_entry(&entry);
        self.entries.insert(key.clone(), entry);
        self.deleted_keys.remove(&key);
        self.dirty_keys.insert(key);
        self.dirty = true;
    }

    pub(crate) fn get(&self, identity: CacheIdentity, path: &Path) -> Option<&CachedSourceEntry> {
        let key = CacheKey::new(identity, path);
        self.entries.get(&key).filter(|entry| {
            entry.parser_namespace == identity.namespace
                && entry.parser_version == identity.parser_version
        })
    }

    pub(crate) fn remove(&mut self, identity: CacheIdentity, path: &Path) {
        let key = CacheKey::new(identity, path);
        if let Some(entry) = self.entries.remove(&key) {
            self.dirty_keys.remove(&key);
            self.deleted_keys
                .insert(key, DeletionReason::Invalidated(entry.fingerprint));
            self.dirty = true;
        }
    }

    pub(crate) fn prune_missing_files(&mut self) {
        let removed_keys: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|key| !key.path.to_path_buf().exists())
            .cloned()
            .collect();

        for key in removed_keys {
            self.entries.remove(&key);
            self.dirty_keys.remove(&key);
            self.deleted_keys.insert(key, DeletionReason::Missing);
            self.dirty = true;
        }
    }
}
