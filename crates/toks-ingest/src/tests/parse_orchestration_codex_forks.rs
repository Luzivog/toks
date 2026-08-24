use super::{codex_fixtures::*, support::*, *};
#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_codex_deduplicates_forked_history() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_codex_forked_history_fixture(source_home.path());

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.input)
                .sum::<i64>(),
            88
        );
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.cache_read)
                .sum::<i64>(),
            22
        );
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.output)
                .sum::<i64>(),
            33
        );
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_codex_keeps_user_fork_own_turn() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_codex_user_fork_replay_fixture(source_home.path());

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        let session_ids: HashSet<_> = messages
            .iter()
            .map(|message| message.session_id.as_str())
            .collect();
        assert!(session_ids
            .contains("rollout-2026-01-02T03-10-00-22222222-2222-7222-8222-222222222222"));
        assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 1000);
        assert_eq!(
            messages.iter().map(|m| m.tokens.cache_read).sum::<i64>(),
            500
        );
        assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 150);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_codex_scans_archived_sessions_without_double_counting() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_codex_sessions_and_archived_sessions_fixture(source_home.path());

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        // live-only + archived-only + shared (counted once, not twice).
        assert_eq!(
            messages.len(),
            3,
            "archived_sessions must be scanned (live-only + archived-only), and a session \
                 present in both sessions/ and archived_sessions/ must be deduplicated to one \
                 message, not counted twice"
        );

        let session_ids: HashSet<_> = messages
            .iter()
            .map(|message| message.session_id.as_str())
            .collect();
        assert!(session_ids.contains("live-only"));
        assert!(
            session_ids.contains("archived-only"),
            "archived_sessions/archived-only.jsonl must be scanned and parsed"
        );

        // 50 (live-only) + 70 (archived-only) + 30 (shared, once) = 150.
        assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 150);
        // 5 (live-only) + 7 (archived-only) + 3 (shared, once) = 15.
        assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 15);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_codex_deduplicates_parent_replay_across_forks() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_codex_parent_replay_fixture(source_home.path());

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        // Parent contributes its two turns. The two forks each replay the
        // parent history (skipped) and then emit one own turn that lands on
        // the identical cumulative total (140/14). Sibling forks sharing a
        // cumulative total is the signature of a replayed row, so the
        // fork-parent-scoped dedup key collapses them into one. Real fork
        // fan-out replays the same upstream totals into 10-100+ siblings;
        // two distinct turns reaching a byte-identical cumulative vector by
        // chance does not happen in practice because the cumulative encodes
        // each fork's divergent context size.
        assert_eq!(messages.len(), 3);
        assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 140);
        assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 14);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_codex_keeps_twin_token_counts_at_distinct_timestamps() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_codex_twin_token_count_fixture(source_home.path());

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(
            messages.len(),
            2,
            "two turns with identical token deltas at distinct timestamps must both survive dedup",
        );
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.input)
                .sum::<i64>(),
            16,
            "input tokens normalize cache_read out of input: 2 turns × (10 - 2) = 16",
        );
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.output)
                .sum::<i64>(),
            6,
        );
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.cache_read)
                .sum::<i64>(),
            4,
        );
    }
}
