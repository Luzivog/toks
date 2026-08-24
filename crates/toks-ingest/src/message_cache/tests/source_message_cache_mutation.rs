use super::*;

fn write_sources_in_same_shard(dir: &TempDir, identity: CacheIdentity) -> (PathBuf, PathBuf) {
    let mut paths_by_shard = HashMap::new();
    for index in 0..=CACHE_SHARD_COUNT * 4 {
        let candidate = dir.path().join(format!("source-{index}.jsonl"));
        std::fs::write(&candidate, format!("source-{index}\n")).unwrap();
        let shard = CacheKey::new(identity, &candidate).shard();
        if let Some(first) = paths_by_shard.insert(shard, candidate.clone()) {
            return (first, candidate);
        }
    }

    panic!("failed to find paths in the same cache shard");
}

#[test]
#[serial_test::serial]
fn test_explicit_invalidation_of_existing_path_persists() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source = write_temp_file(b"still exists\n");
    let identity = CacheIdentity::for_client(ClientId::Claude);

    let mut seed = SourceMessageCache::default();
    seed.insert(test_entry(identity, source.path(), "session-1"));
    seed.save_if_dirty();
    assert!(SourceMessageCache::load()
        .get(identity, source.path())
        .is_some());

    let mut cache = SourceMessageCache::load();
    cache.remove(identity, source.path());
    cache.save_if_dirty();

    assert!(
        source.path().is_file(),
        "invalidation must not remove the source"
    );
    assert!(SourceMessageCache::load()
        .get(identity, source.path())
        .is_none());
}

#[test]
#[serial_test::serial]
fn test_stale_invalidation_preserves_concurrently_refreshed_entry() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source_dir = TempDir::new().unwrap();
    let path = source_dir.path().join("session.jsonl");
    let identity = CacheIdentity::for_client(ClientId::Claude);
    std::fs::write(&path, b"old\n").unwrap();

    let mut seed = SourceMessageCache::default();
    seed.insert(test_entry(identity, &path, "old-session"));
    seed.save_if_dirty();

    let mut stale_invalidator = SourceMessageCache::load();
    stale_invalidator.remove(identity, &path);

    std::fs::write(&path, b"fresh-content\n").unwrap();
    let mut fresh_writer = SourceMessageCache::load();
    fresh_writer.insert(test_entry(identity, &path, "fresh-session"));
    fresh_writer.save_if_dirty();

    stale_invalidator.save_if_dirty();

    let loaded = SourceMessageCache::load();
    assert_eq!(
        loaded.get(identity, &path).unwrap().messages[0].session_id,
        "fresh-session"
    );
}

#[test]
fn test_prune_missing_files_removes_deleted_entries() {
    let file = write_temp_file(b"{}\n");
    let path = file.path().to_path_buf();
    let identity = CacheIdentity::for_client(ClientId::Claude);

    let mut cache = SourceMessageCache::default();
    cache.insert(test_entry(identity, &path, "session-1"));

    std::fs::remove_file(&path).unwrap();
    cache.prune_missing_files();

    assert!(cache.entries.is_empty());
}

#[test]
#[serial_test::serial]
fn test_save_if_dirty_marks_cache_clean() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    let mut cache = SourceMessageCache::default();
    assert!(!cache.dirty);

    {
        let file = write_temp_file(b"{}\n");
        let identity = CacheIdentity::for_client(ClientId::Claude);
        cache.insert(test_entry(identity, file.path(), "session-1"));
        assert!(cache.dirty);

        cache.save_if_dirty();
        assert!(!cache.dirty);
    }
}

#[test]
#[serial_test::serial]
fn test_save_if_dirty_merges_concurrent_writers() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let identity = CacheIdentity::for_client(ClientId::Claude);
        let (path_one, path_two) = write_sources_in_same_shard(&source_dir, identity);
        assert_eq!(
            CacheKey::new(identity, &path_one).shard(),
            CacheKey::new(identity, &path_two).shard()
        );

        let mut writer_one = SourceMessageCache::load();
        let mut writer_two = SourceMessageCache::load();

        writer_one.insert(test_entry(identity, &path_one, "session-1"));
        writer_two.insert(test_entry(identity, &path_two, "session-2"));

        writer_one.save_if_dirty();
        writer_two.save_if_dirty();

        let loaded = SourceMessageCache::load();
        assert!(loaded.get(identity, &path_one).is_some());
        assert!(loaded.get(identity, &path_two).is_some());
    }
}

#[test]
#[serial_test::serial]
fn test_save_if_dirty_preserves_recreated_path_from_concurrent_writer() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let path = source_dir.path().join("session.jsonl");
        std::fs::write(&path, b"{\"id\":\"old\"}\n").unwrap();
        let identity = CacheIdentity::for_client(ClientId::Claude);

        let mut seed = SourceMessageCache::default();
        seed.insert(test_entry(identity, &path, "old-session"));
        seed.save_if_dirty();

        let mut stale_deleter = SourceMessageCache::load();
        std::fs::remove_file(&path).unwrap();
        stale_deleter.prune_missing_files();

        std::fs::write(&path, b"{\"id\":\"fresh\"}\n").unwrap();
        let mut fresh_writer = SourceMessageCache::load();
        fresh_writer.insert(test_entry(identity, &path, "fresh-session"));
        fresh_writer.save_if_dirty();

        stale_deleter.save_if_dirty();

        let loaded = SourceMessageCache::load();
        let entry = loaded
            .get(identity, &path)
            .expect("recreated source cache entry should survive stale delete");
        assert_eq!(entry.messages[0].session_id, "fresh-session");
    }
}
