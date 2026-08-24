use super::*;

fn write_sources_in_distinct_shards(dir: &TempDir, identity: CacheIdentity) -> (PathBuf, PathBuf) {
    let first = dir.path().join("source-0.jsonl");
    std::fs::write(&first, b"source-0\n").unwrap();
    let first_shard = CacheKey::new(identity, &first).shard();

    for index in 1..=CACHE_SHARD_COUNT * 2 {
        let candidate = dir.path().join(format!("source-{index}.jsonl"));
        std::fs::write(&candidate, format!("source-{index}\n")).unwrap();
        if CacheKey::new(identity, &candidate).shard() != first_shard {
            return (first, candidate);
        }
    }

    panic!("failed to find paths in distinct cache shards");
}

#[test]
fn test_write_shard_round_trips_after_atomic_replace() {
    let source = write_temp_file(b"{}\n");
    let identity = CacheIdentity::for_client(ClientId::Claude);
    let entry = test_entry(identity, source.path(), "session-1");
    let shard_dir = TempDir::new().unwrap();
    let shard_path = shard_dir.path().join("shard.bin");

    write_shard_with_limit(
        &shard_path,
        identity,
        std::slice::from_ref(&entry),
        MAX_CACHE_SHARD_BYTES,
    )
    .unwrap();

    assert!(matches!(
        read_shard(&shard_path, identity),
        ShardReadStatus::Loaded(entries)
            if entries.len() == 1 && entries[0].messages[0].session_id == "session-1"
    ));
}

#[test]
#[serial_test::serial]
fn test_source_message_cache_round_trips_across_distinct_shards() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source_dir = TempDir::new().unwrap();
    let identity = CacheIdentity::for_client(ClientId::Claude);
    let (path_one, path_two) = write_sources_in_distinct_shards(&source_dir, identity);
    let shard_one = cache_shard_path(identity, &path_one);
    let shard_two = cache_shard_path(identity, &path_two);
    assert_ne!(shard_one, shard_two);

    let mut cache = SourceMessageCache::default();
    cache.insert(test_entry(identity, &path_one, "session-1"));
    cache.insert(test_entry(identity, &path_two, "session-2"));
    cache.save_if_dirty();

    assert!(shard_one.is_file());
    assert!(shard_two.is_file());
    let loaded = SourceMessageCache::load();
    assert_eq!(loaded.entries.len(), 2);
    assert!(loaded.get(identity, &path_one).is_some());
    assert!(loaded.get(identity, &path_two).is_some());
}

#[test]
#[serial_test::serial]
fn requested_clients_load_without_deserializing_unrelated_namespaces() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let sources = TempDir::new().unwrap();
    let claude_path = sources.path().join("claude.jsonl");
    let codex_path = sources.path().join("codex.jsonl");
    let unrelated_path = sources.path().join("opencode.jsonl");
    for path in [&claude_path, &codex_path, &unrelated_path] {
        std::fs::write(path, b"{}\n").unwrap();
    }

    let claude = CacheIdentity::for_client(ClientId::Claude);
    let codex = CacheIdentity::for_client(ClientId::Codex);
    let unrelated = CacheIdentity::for_client(ClientId::OpenCode);
    let mut cache = SourceMessageCache::default();
    cache.insert(test_entry(claude, &claude_path, "retained-claude"));
    cache.insert(test_entry(codex, &codex_path, "incremental-codex"));
    cache.insert(test_entry(unrelated, &unrelated_path, "unrelated"));
    cache.save_if_dirty();

    let requested =
        SourceMessageCache::load_for_clients(&["claude".to_string(), "codex".to_string()]);
    assert_eq!(requested.entries.len(), 2);
    assert_eq!(
        requested.get(claude, &claude_path).unwrap().messages[0].session_id,
        "retained-claude"
    );
    assert!(requested.get(codex, &codex_path).is_some());
    assert!(requested.get(unrelated, &unrelated_path).is_none());
}

#[test]
#[serial_test::serial]
fn test_aggregate_cache_can_exceed_individual_shard_limit() {
    const TEST_SHARD_LIMIT: u64 = 32 * 1024;

    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source_dir = TempDir::new().unwrap();
    let identity = CacheIdentity::for_client(ClientId::Claude);
    let (path_one, path_two) = write_sources_in_distinct_shards(&source_dir, identity);

    let mut entry_one = test_entry(identity, &path_one, "session-1");
    entry_one.messages[0].model_id = "a".repeat(20 * 1024);
    let mut entry_two = test_entry(identity, &path_two, "session-2");
    entry_two.messages[0].model_id = "b".repeat(20 * 1024);

    let mut cache = SourceMessageCache::default();
    cache.insert(entry_one);
    cache.insert(entry_two);
    cache.save_if_dirty_with_limit(TEST_SHARD_LIMIT);
    assert!(
        !cache.dirty,
        "both independently bounded shards should save"
    );

    let shard_one = cache_shard_path(identity, &path_one);
    let shard_two = cache_shard_path(identity, &path_two);
    let size_one = std::fs::metadata(&shard_one).unwrap().len();
    let size_two = std::fs::metadata(&shard_two).unwrap().len();
    assert!(size_one <= TEST_SHARD_LIMIT);
    assert!(size_two <= TEST_SHARD_LIMIT);
    assert!(size_one + size_two > TEST_SHARD_LIMIT);

    let loaded = SourceMessageCache::load();
    assert!(loaded.get(identity, &path_one).is_some());
    assert!(loaded.get(identity, &path_two).is_some());
}

#[test]
#[serial_test::serial]
fn test_corrupt_shard_does_not_hide_entries_from_other_shards() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source_dir = TempDir::new().unwrap();
    let identity = CacheIdentity::for_client(ClientId::Claude);
    let (corrupt_path, valid_path) = write_sources_in_distinct_shards(&source_dir, identity);

    let mut cache = SourceMessageCache::default();
    cache.insert(test_entry(identity, &corrupt_path, "corrupt-session"));
    cache.insert(test_entry(identity, &valid_path, "valid-session"));
    cache.save_if_dirty();

    let corrupt_shard = cache_shard_path(identity, &corrupt_path);
    std::fs::write(&corrupt_shard, b"not a bincode shard").unwrap();
    assert!(matches!(
        read_shard(&corrupt_shard, identity),
        ShardReadStatus::Invalid(_)
    ));

    let loaded = SourceMessageCache::load();
    assert!(loaded.get(identity, &corrupt_path).is_none());
    assert_eq!(
        loaded.get(identity, &valid_path).unwrap().messages[0].session_id,
        "valid-session"
    );
    assert!(
        loaded.dirty,
        "the corrupt shard should be scheduled for rewrite"
    );
}

#[test]
#[serial_test::serial]
fn test_stale_parser_shard_is_skipped_before_decoding_garbage_payload() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source = write_temp_file(b"claude\n");
    let claude = CacheIdentity::for_client(ClientId::Claude);
    let codex = CacheIdentity::for_client(ClientId::Codex);

    let mut seed = SourceMessageCache::default();
    seed.insert(test_entry(claude, source.path(), "claude-session"));
    seed.save_if_dirty();

    let stale_key = CacheShardKey {
        namespace: codex.namespace.to_string(),
        index: 0,
    };
    let stale_path = shard_path(&cache_shard_dir().unwrap(), &stale_key);
    ensure_cache_dir(stale_path.parent().unwrap()).unwrap();
    let stale_envelope = CachedShardEnvelope {
        format_version: CACHE_FORMAT_VERSION,
        parser_namespace: codex.namespace.to_string(),
        parser_version: codex.parser_version.saturating_sub(1),
        payload: b"deliberately invalid entry payload".to_vec(),
    };
    // Scoped, so the handle is closed before anything rewrites this shard:
    // the rewrite goes through an atomic replace, and Windows refuses to
    // replace a file another handle still has open (`Access is denied`, os
    // error 5). On Unix the rename succeeds with the handle open, which is
    // why the leak was invisible.
    {
        let mut writer = BufWriter::new(File::create(&stale_path).unwrap());
        bincode::options()
            .serialize_into(&mut writer, &stale_envelope)
            .unwrap();
        writer.flush().unwrap();
    }

    assert!(matches!(
        read_shard(&stale_path, codex),
        ShardReadStatus::Stale
    ));
    let mut loaded = SourceMessageCache::load();
    assert_eq!(loaded.entries.len(), 1);
    assert!(loaded.get(claude, source.path()).is_some());
    assert!(loaded.rewrite_shards.contains(&stale_key));

    loaded.save_if_dirty();
    assert!(matches!(
        read_shard(&stale_path, codex),
        ShardReadStatus::Loaded(entries) if entries.is_empty()
    ));
    assert!(SourceMessageCache::load()
        .get(claude, source.path())
        .is_some());
}

#[test]
#[serial_test::serial]
fn test_prior_cache_format_shard_is_skipped_before_decoding_payload() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let codex = CacheIdentity::for_client(ClientId::Codex);
    let stale_key = CacheShardKey {
        namespace: codex.namespace.to_string(),
        index: 0,
    };
    let stale_path = shard_path(&cache_shard_dir().unwrap(), &stale_key);
    ensure_cache_dir(stale_path.parent().unwrap()).unwrap();
    let stale_envelope = CachedShardEnvelope {
        format_version: FORMAT_V4 - 1,
        parser_namespace: codex.namespace.to_string(),
        parser_version: codex.parser_version,
        payload: b"prior UnifiedMessage layout".to_vec(),
    };
    let mut writer = BufWriter::new(File::create(&stale_path).unwrap());
    bincode::options()
        .serialize_into(&mut writer, &stale_envelope)
        .unwrap();
    writer.flush().unwrap();

    assert!(matches!(
        read_shard(&stale_path, codex),
        ShardReadStatus::Stale
    ));
}

#[test]
#[serial_test::serial]
fn test_v4_shard_migrates_messages_and_rewrites_once() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source = write_temp_file(b"{}\n");
    let identity = CacheIdentity::for_client(ClientId::PrimeAgent);
    let entry = test_entry(identity, source.path(), "legacy-prime");
    let key = CacheKey::from_entry(&entry);
    let shard_key = key.shard();
    let legacy_path = shard_path(&cache_shard_dir().unwrap(), &shard_key);
    ensure_cache_dir(legacy_path.parent().unwrap()).unwrap();
    let legacy_entry = LegacyCachedSourceEntryV4 {
        parser_namespace: entry.parser_namespace,
        parser_version: entry.parser_version,
        path: entry.path,
        fingerprint: entry.fingerprint,
        messages: entry
            .messages
            .into_iter()
            .map(LegacyUnifiedMessageV5::from)
            .collect(),
        fallback_timestamp_indices: entry.fallback_timestamp_indices,
        codex_incremental: entry.codex_incremental.map(Into::into),
    };
    let envelope = CachedShardEnvelope {
        format_version: FORMAT_V4,
        parser_namespace: identity.namespace.to_string(),
        parser_version: identity.parser_version,
        payload: bincode::options().serialize(&vec![legacy_entry]).unwrap(),
    };
    let mut writer = BufWriter::new(File::create(&legacy_path).unwrap());
    bincode::options()
        .serialize_into(&mut writer, &envelope)
        .unwrap();
    writer.flush().unwrap();
    drop(writer);

    assert!(matches!(
        read_shard(&legacy_path, identity),
        ShardReadStatus::Migrated(entries)
            if entries.len() == 1
                && entries[0].messages[0].session_id == "legacy-prime"
                && entries[0].prime_accounting.is_none()
    ));

    let mut cache = SourceMessageCache::load();
    assert_eq!(
        cache.get(identity, source.path()).unwrap().messages[0].session_id,
        "legacy-prime"
    );
    assert!(cache.rewrite_shards.contains(&shard_key));
    cache.save_if_dirty();
    assert!(matches!(
        read_shard(&legacy_path, identity),
        ShardReadStatus::Loaded(entries)
            if entries.len() == 1 && entries[0].prime_accounting.is_none()
    ));
}
