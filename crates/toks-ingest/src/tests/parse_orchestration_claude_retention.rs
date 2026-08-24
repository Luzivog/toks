use super::{support::*, *};
/// Claude Code rewrites a session transcript in place on resume/compact:
/// the file keeps its path and session id but loses already-written
/// assistant turns. Because the source cache tracks live file content, a
/// rescan after such a rewrite used to drop those turns from history for
/// good — and `tokscope submit` feeds the leaderboard from the same
/// recompute. See https://github.com/junhoyeo/tokscope/issues/994.
#[test]
#[serial_test::serial]
fn test_claude_in_place_rewrite_preserves_previously_seen_messages() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let claude_dir = source_home.path().join(".claude/projects/myproject");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let transcript = claude_dir.join("conversation.jsonl");

        let turn_one = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":7,"cache_creation_input_tokens":3}}}"#;
        let turn_two = r#"{"type":"assistant","timestamp":"2024-12-01T10:05:00.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":60,"cache_read_input_tokens":11,"cache_creation_input_tokens":5}}}"#;
        let turn_three = r#"{"type":"assistant","timestamp":"2024-12-01T10:10:00.000Z","requestId":"req_003","message":{"id":"msg_003","model":"claude-3-5-sonnet","usage":{"input_tokens":300,"output_tokens":70,"cache_read_input_tokens":13,"cache_creation_input_tokens":17}}}"#;

        std::fs::write(
            &transcript,
            format!("{turn_one}\n{turn_two}\n{turn_three}\n"),
        )
        .unwrap();

        let before = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(before.len(), 3, "cold scan must see all three turns");
        let before_output: i64 = before.iter().map(|m| m.tokens.output).sum();
        let before_cache_read: i64 = before.iter().map(|m| m.tokens.cache_read).sum();
        let before_cache_write: i64 = before.iter().map(|m| m.tokens.cache_write).sum();
        assert_eq!(before_output, 180);

        // The rewrite: same path, same session, two assistant turns gone.
        std::fs::write(&transcript, format!("{turn_three}\n")).unwrap();

        let after = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );

        assert_eq!(
            after.len(),
            3,
            "an in-place rewrite must not retire messages the cache already observed"
        );
        assert_eq!(
            after.iter().map(|m| m.tokens.output).sum::<i64>(),
            before_output
        );
        assert_eq!(
            after.iter().map(|m| m.tokens.cache_read).sum::<i64>(),
            before_cache_read
        );
        assert_eq!(
            after.iter().map(|m| m.tokens.cache_write).sum::<i64>(),
            before_cache_write
        );

        // Retention has to survive its own round trip through the cache:
        // the rewritten entry is what the NEXT scan reads back, so a
        // union that is computed but not persisted drifts one run later.
        let third = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(
            third.len(),
            3,
            "the retained turns must be written back to the cache, not just returned once"
        );
        assert_eq!(
            third.iter().map(|m| m.tokens.output).sum::<i64>(),
            before_output
        );
    }
}

/// Retention is only sound because a retained message still collapses
/// against a live copy of itself somewhere else. Claude Code forks a
/// session into a new transcript that replays earlier turns, so the same
/// `messageId:requestId` legitimately appears in two files at once.
///
/// The transcripts are named so the retaining file sorts first: the lane
/// walks paths in lexical order and keeps the first copy of a key, so this
/// is the ordering where a retained copy wins over the live one. Both
/// copies come from the same API response, so either winner reports the
/// same tokens — what must not happen is both being counted.
#[test]
#[serial_test::serial]
fn test_claude_retained_message_collapses_against_a_forked_transcript() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let claude_dir = source_home.path().join(".claude/projects/myproject");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let original = claude_dir.join("aaa-original.jsonl");

        let turn_one = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
        let turn_two = r#"{"type":"assistant","timestamp":"2024-12-01T10:05:00.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":60}}}"#;
        let turn_three = r#"{"type":"assistant","timestamp":"2024-12-01T10:10:00.000Z","requestId":"req_003","message":{"id":"msg_003","model":"claude-3-5-sonnet","usage":{"input_tokens":300,"output_tokens":70}}}"#;

        std::fs::write(&original, format!("{turn_one}\n{turn_two}\n{turn_three}\n")).unwrap();
        let before = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(before.len(), 3);

        // The compaction keeps turn one. Turn two is replayed into the
        // fork, so it exists both retained and live. Turn three exists
        // only as a retained copy — it is what makes the count here
        // differ from a run with no retention at all.
        std::fs::write(&original, format!("{turn_one}\n")).unwrap();
        std::fs::write(claude_dir.join("zzz-fork.jsonl"), format!("{turn_two}\n")).unwrap();

        let after = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(
            after.len(),
            3,
            "retention must keep turn three and must not double count turn two"
        );
        let keys: HashSet<String> = after
            .iter()
            .filter_map(|message| message.dedup_key.clone())
            .collect();
        assert_eq!(keys.len(), 3, "every surviving message must be distinct");
        assert_eq!(
            after.iter().map(|m| m.tokens.output).sum::<i64>(),
            180,
            "turn two contributes its 60 once, not twice"
        );
        assert_eq!(after.iter().map(|m| m.tokens.input).sum::<i64>(), 600);
    }
}

/// A Claude tool-result key embeds the session id, which is the
/// transcript's file stem. A retained tool result therefore could never
/// collapse against the same tool result replayed under a fork's filename
/// — both would count — so retention has to leave those records behind
/// even though it means a compaction still retires their input tokens.
#[test]
#[serial_test::serial]
fn test_claude_path_scoped_tool_result_is_not_retained_across_a_rewrite() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let claude_dir = source_home.path().join(".claude/projects/myproject");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let transcript = claude_dir.join("conversation.jsonl");

        let assistant_turn = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
        let tool_result = r#"{"type":"user","timestamp":"2024-12-01T10:01:00.000Z","message":{"model":"claude-3-5-sonnet","content":[{"type":"tool_result","tool_use_id":"toolu_1","tool_output":{"input_tokens":40,"output":"result"}}]}}"#;

        std::fs::write(&transcript, format!("{assistant_turn}\n{tool_result}\n")).unwrap();
        let before = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(
            before.len(),
            2,
            "cold scan sees the turn and the tool result"
        );
        assert_eq!(before.iter().map(|m| m.tokens.input).sum::<i64>(), 140);

        // The rewrite drops both records; only the assistant turn is
        // re-added, so the tool result is a candidate for retention.
        std::fs::write(&transcript, format!("{assistant_turn}\n")).unwrap();

        let after = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(
            after.len(),
            1,
            "a path-scoped key must not be carried across the rewrite"
        );
        assert_eq!(after.iter().map(|m| m.tokens.input).sum::<i64>(), 100);
    }
}

/// The retention above must not resurrect a session the user deleted:
/// `prune_missing_files` drops the entry when the file is gone, which is
/// the behavior `d9df8c9c` (local session cleanup) depends on.
///
/// The transcript is compacted first, and retention is asserted, so the
/// deletion runs against an entry that really is holding a turn the live
/// file no longer has. Deleting straight after a cold scan would prove
/// nothing about retention.
#[test]
#[serial_test::serial]
fn test_claude_deleted_transcript_is_not_resurrected_by_retention() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let claude_dir = source_home.path().join(".claude/projects/myproject");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let transcript = claude_dir.join("conversation.jsonl");

        let turn_one = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
        let turn_two = r#"{"type":"assistant","timestamp":"2024-12-01T10:05:00.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":60}}}"#;

        std::fs::write(&transcript, format!("{turn_one}\n{turn_two}\n")).unwrap();
        let before = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(before.len(), 2);

        std::fs::write(&transcript, format!("{turn_two}\n")).unwrap();
        let retained = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(
            retained.len(),
            2,
            "the entry must actually be holding a retained turn before the delete"
        );

        std::fs::remove_file(&transcript).unwrap();

        let after = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
            false,
            &scanner::ScannerSettings::default(),
        );
        assert!(
                after.is_empty(),
                "a deleted transcript stays deleted, retained turns and all; local disk remains the source of truth"
            );
    }
}

#[test]
#[serial_test::serial]
fn test_claude_warm_cache_removes_synthetic_placeholder_before_submit_validation() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let claude_dir = client_scan_root(source_home.path(), ClientId::Claude).join("demo");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let transcript = claude_dir.join("session.jsonl");
        std::fs::write(
                &transcript,
                r#"{"type":"assistant","timestamp":"2026-06-24T01:00:00.000Z","requestId":"req_live","message":{"id":"live","model":"claude-3-5-sonnet","usage":{"input_tokens":1,"output_tokens":1}}}"#,
            )
            .unwrap();

        let identity = message_cache::CacheIdentity::for_client(ClientId::Claude);
        let fingerprint = message_cache::SourceFingerprint::from_claude_code_path_with_home(
            &transcript,
            Some(source_home.path()),
        )
        .unwrap();
        let retained = UnifiedMessage::new_with_dedup(
            "claude",
            "claude-3-5-sonnet",
            "anthropic",
            "session",
            1_782_259_200_000,
            TokenBreakdown {
                input: 10,
                output: 5,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
            Some("old:req_old".to_string()),
        );
        let poisoned = UnifiedMessage::new_with_dedup(
            "claude",
            "<synthetic>",
            "unknown",
            "session",
            1_782_259_201_000,
            TokenBreakdown {
                input: 100,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
            Some("claude:tool_result:session:tool_result:toolu_1".to_string()),
        );
        let mut cache = message_cache::SourceMessageCache::default();
        cache.insert(message_cache::CachedSourceEntry::new(
            identity,
            &transcript,
            fingerprint,
            vec![retained, poisoned],
            Vec::new(),
            None,
        ));
        cache.save_if_dirty();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "claude-3-5-sonnet");
        assert_eq!(messages[0].tokens.input, 10);

        let repaired = message_cache::SourceMessageCache::load();
        let cached = repaired
            .get(identity, &transcript)
            .expect("the retained Claude cache entry should remain");
        assert_eq!(cached.messages.len(), 1);
        assert_eq!(cached.messages[0].dedup_key.as_deref(), Some("old:req_old"));
    }
}
