use super::*;

fn keyed_message(namespace: &str, session_id: &str, dedup_key: &str) -> UnifiedMessage {
    UnifiedMessage::new_with_dedup(
        namespace,
        "claude-3-5-sonnet",
        "anthropic",
        session_id,
        1,
        TokenBreakdown {
            input: 1,
            output: 2,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(dedup_key.to_string()),
    )
}

fn entry_with_messages(
    identity: CacheIdentity,
    path: &Path,
    messages: Vec<UnifiedMessage>,
) -> CachedSourceEntry {
    CachedSourceEntry::new(
        identity,
        path,
        SourceFingerprint::from_path(path).unwrap(),
        messages,
        Vec::new(),
        None,
    )
}

fn synthetic_placeholder_message(session_id: &str, dedup_key: &str) -> UnifiedMessage {
    let mut message = keyed_message(ClientId::Claude.as_str(), session_id, dedup_key);
    message.model_id = " <SYNTHETIC> ".to_string();
    message.provider_id = "unknown".to_string();
    message
}

#[test]
#[serial_test::serial]
fn test_loading_claude_cache_removes_synthetic_placeholder_rows_without_retiring_history() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let path = source_dir.path().join("conversation.jsonl");
        std::fs::write(&path, b"{\"id\":\"live\"}\n").unwrap();
        let identity = CacheIdentity::for_client(ClientId::Claude);
        let real_live = keyed_message("claude", "session", "live:req_live");
        let real_retained = keyed_message("claude", "session", "old:req_old");
        let synthetic_assistant =
            synthetic_placeholder_message("session", "synthetic:req_synthetic");
        let mut synthetic_tool_result = synthetic_placeholder_message(
            "session",
            "claude:tool_result:conversation:tool_result:toolu_1",
        );
        synthetic_tool_result.tokens.input = 100;

        let mut seed = SourceMessageCache::default();
        seed.insert(entry_with_messages(
            identity,
            &path,
            vec![
                real_live.clone(),
                real_retained.clone(),
                synthetic_assistant,
                synthetic_tool_result,
            ],
        ));
        seed.save_if_dirty();

        let mut repaired = SourceMessageCache::load();
        let entry = repaired
            .get(identity, &path)
            .expect("current Claude cache entry should load");
        assert_eq!(entry.messages.len(), 2);
        assert_eq!(
            entry
                .messages
                .iter()
                .filter_map(|message| message.dedup_key.as_deref())
                .collect::<HashSet<_>>(),
            HashSet::from(["live:req_live", "old:req_old"]),
            "the targeted migration must retain real live and compacted history"
        );
        repaired.save_if_dirty();

        let shard_path = cache_shard_path(identity, &path);
        assert!(matches!(
            read_shard(&shard_path, identity),
            ShardReadStatus::Loaded(entries)
                if entries.len() == 1
                    && entries[0].messages.len() == 2
                    && entries[0]
                        .messages
                        .iter()
                        .all(|message| message.model_id != " <SYNTHETIC> ")
        ));
    }
}

#[test]
#[serial_test::serial]
fn test_claude_cache_save_does_not_restore_synthetic_history_from_another_writer() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let path = source_dir.path().join("conversation.jsonl");
        std::fs::write(&path, b"{\"id\":\"live\"}\n").unwrap();
        let identity = CacheIdentity::for_client(ClientId::Claude);
        let real = keyed_message("claude", "session", "live:req_live");
        let synthetic = synthetic_placeholder_message("session", "synthetic:req_synthetic");

        let mut seed = SourceMessageCache::default();
        seed.insert(entry_with_messages(
            identity,
            &path,
            vec![real.clone(), synthetic],
        ));
        seed.save_if_dirty();

        // Simulate a process that parsed the live source after a compaction
        // while an old on-disk entry still carries the synthetic notice.
        // The normal retained-history merge would bring the globally stable
        // synthetic key back, so sanitation must run after that merge too.
        let mut fresh_writer = SourceMessageCache::default();
        fresh_writer.insert(entry_with_messages(identity, &path, vec![real]));
        fresh_writer.save_if_dirty();

        let shard_path = cache_shard_path(identity, &path);
        assert!(matches!(
            read_shard(&shard_path, identity),
            ShardReadStatus::Loaded(entries)
                if entries.len() == 1
                    && entries[0].messages.len() == 1
                    && entries[0].messages[0].dedup_key.as_deref() == Some("live:req_live")
        ));
    }
}

/// A Claude entry can hold assistant turns the live transcript no longer
/// contains (an in-place compaction dropped them). Two processes scanning
/// at once therefore hold genuinely different histories for one path, and
/// the wholesale last-writer-wins replace would retire the loser's turns
/// for good — the live file cannot hand them back.
#[test]
#[serial_test::serial]
fn test_save_if_dirty_unions_retained_history_for_the_same_path() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let path = source_dir.path().join("conversation.jsonl");
        std::fs::write(&path, b"{\"id\":\"live\"}\n").unwrap();
        let identity = CacheIdentity::for_client(ClientId::Claude);
        let namespace = ClientId::Claude.as_str();

        // Both processes carry the turn the file still has. Only the first
        // ever observed the one a compaction later removed.
        let shared = keyed_message(namespace, "session", "msg_shared:req_shared");
        let observed_only_by_first = keyed_message(namespace, "session", "msg_dropped:req_dropped");

        let mut observer = SourceMessageCache::load();
        observer.insert(entry_with_messages(
            identity,
            &path,
            vec![shared.clone(), observed_only_by_first],
        ));
        observer.save_if_dirty();

        let mut latecomer = SourceMessageCache::load();
        latecomer.insert(entry_with_messages(identity, &path, vec![shared]));
        latecomer.save_if_dirty();

        let loaded = SourceMessageCache::load();
        let entry = loaded.get(identity, &path).expect("entry should survive");
        let keys: HashSet<&str> = entry
            .messages
            .iter()
            .filter_map(|message| message.dedup_key.as_deref())
            .collect();
        assert!(
            keys.contains("msg_dropped:req_dropped"),
            "a concurrent writer must not discard history it never saw, got {keys:?}"
        );
        assert_eq!(
            entry.messages.len(),
            2,
            "and must not duplicate the shared turn"
        );
    }
}

/// The union is scoped to keys that stay valid wherever the message is
/// written. A Claude tool-result key embeds the transcript's file stem, so
/// a carried-forward copy could never collapse against the same tool
/// result replayed into a forked transcript — both would count.
#[test]
#[serial_test::serial]
fn test_save_if_dirty_does_not_union_path_scoped_keys() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let path = source_dir.path().join("conversation.jsonl");
        std::fs::write(&path, b"{\"id\":\"live\"}\n").unwrap();
        let identity = CacheIdentity::for_client(ClientId::Claude);
        let namespace = ClientId::Claude.as_str();

        let shared = keyed_message(namespace, "session", "msg_shared:req_shared");
        let tool_result = keyed_message(
            namespace,
            "session",
            "claude:tool_result:conversation:tool_result:toolu_1",
        );

        let mut observer = SourceMessageCache::load();
        observer.insert(entry_with_messages(
            identity,
            &path,
            vec![shared.clone(), tool_result],
        ));
        observer.save_if_dirty();

        let mut latecomer = SourceMessageCache::load();
        latecomer.insert(entry_with_messages(identity, &path, vec![shared]));
        latecomer.save_if_dirty();

        let loaded = SourceMessageCache::load();
        let entry = loaded.get(identity, &path).expect("entry should survive");
        assert_eq!(
            entry.messages.len(),
            1,
            "path-scoped keys must not outlive the bytes that produced them"
        );
    }
}

/// The union exists only for namespaces that retain history. Everywhere
/// else the live file is the whole truth, so a stale entry must still be
/// replaced outright rather than accumulating.
#[test]
#[serial_test::serial]
fn test_save_if_dirty_still_replaces_entries_for_non_retaining_clients() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());

    {
        let source_dir = TempDir::new().unwrap();
        let path = source_dir.path().join("rollout.jsonl");
        std::fs::write(&path, b"{\"id\":\"live\"}\n").unwrap();
        let identity = CacheIdentity::for_client(ClientId::Codex);
        let namespace = ClientId::Codex.as_str();

        let mut observer = SourceMessageCache::load();
        observer.insert(entry_with_messages(
            identity,
            &path,
            vec![
                keyed_message(namespace, "session", "codex-key-a"),
                keyed_message(namespace, "session", "codex-key-b"),
            ],
        ));
        observer.save_if_dirty();

        let mut latecomer = SourceMessageCache::load();
        latecomer.insert(entry_with_messages(
            identity,
            &path,
            vec![keyed_message(namespace, "session", "codex-key-a")],
        ));
        latecomer.save_if_dirty();

        let loaded = SourceMessageCache::load();
        let entry = loaded.get(identity, &path).expect("entry should survive");
        assert_eq!(entry.messages.len(), 1);
    }
}
