use super::*;

#[test]
fn test_forked_child_skips_nested_parent_replay_until_own_turn() {
    let parent = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:51:57.991Z","type":"session_meta","payload":{"id":"019e5b00-0000-7000-8000-000000000001","source":"vscode","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.000Z","type":"turn_context","payload":{"turn_id":"019e5b00-0001-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n"
    ));
    let child = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5c03-1e99-7000-8000-000000000001","forked_from_id":"019e5b00-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019e5b00-0000-7000-8000-000000000001","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5b00-0000-7000-8000-000000000001","source":"vscode","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.100Z","type":"turn_context","payload":{"turn_id":"019e5b00-0001-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"019e5c03-6425-7000-8000-000000000001"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.100Z","type":"turn_context","payload":{"turn_id":"019e5c03-6425-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":320,"output_tokens":32,"total_tokens":352},"last_token_usage":{"input_tokens":20,"output_tokens":2,"total_tokens":22}}}}"#,
        "\n"
    ));

    let parent_messages = parse_codex_file(parent.path());
    let child_messages = parse_codex_file(child.path());

    assert_eq!(parent_messages.len(), 1);
    assert_eq!(child_messages.len(), 1);
    assert_ne!(parent_messages[0].dedup_key, child_messages[0].dedup_key);
    assert_eq!(child_messages[0].tokens.input, 20);
    assert_eq!(child_messages[0].tokens.output, 2);
}

#[test]
fn test_forked_child_same_millisecond_turn_starts_own_session() {
    // Regression: the child's own first turn starts in the SAME millisecond
    // as its fork session_meta, so both UUID v7 ids share the 48-bit ms
    // prefix (`019e5c03-1e99`) and differ only in the random tail
    // (`…0001` vs `…00ff`). Comparing the full id makes the gate fall through
    // to the coin-flip tail (here `0001 < 00ff`), so the replay-skip never
    // ends and the child's own turn is dropped. Comparing only the ms prefix
    // keeps the child's own turn.
    let child = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5c03-1e99-7000-8000-0000000000ff","forked_from_id":"019e5b00-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019e5b00-0000-7000-8000-000000000001","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5b00-0000-7000-8000-000000000001","source":"vscode","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.100Z","type":"turn_context","payload":{"turn_id":"019e5b00-0001-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"019e5c03-1e99-7000-8000-000000000001"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.100Z","type":"turn_context","payload":{"turn_id":"019e5c03-1e99-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":320,"output_tokens":32,"total_tokens":352},"last_token_usage":{"input_tokens":20,"output_tokens":2,"total_tokens":22}}}}"#,
        "\n"
    ));

    let child_messages = parse_codex_file(child.path());

    assert_eq!(child_messages.len(), 1);
    assert_eq!(child_messages[0].tokens.input, 20);
    assert_eq!(child_messages[0].tokens.output, 2);
}

#[test]
fn test_forked_child_same_millisecond_replayed_parent_turn_keeps_skipping() {
    // A replayed parent `turn_context` can coincidentally share the child's
    // fork millisecond (here both `019e5c03-1e99`) while NOT being preceded
    // by a `task_started`. A millisecond-prefix-only gate would treat that
    // equal-prefix turn as child-local, end the skip early, and count the
    // inherited replayed row (500/50) as the child's own usage. The child's
    // own turn is the later one announced by `task_started`; only it should
    // end the skip, so only its 20/2 delta is counted.
    let child = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5c03-1e99-7000-8000-0000000000ff","forked_from_id":"019e5b00-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019e5b00-0000-7000-8000-000000000001","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5b00-0000-7000-8000-000000000001","source":"vscode","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        // replayed parent turn that shares the child's fork millisecond, with
        // NO task_started — must NOT end the skip.
        r#"{"timestamp":"2026-05-05T21:52:10.100Z","type":"turn_context","payload":{"turn_id":"019e5c03-1e99-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":500,"output_tokens":50,"total_tokens":550},"last_token_usage":{"input_tokens":500,"output_tokens":50,"total_tokens":550}}}}"#,
        "\n",
        // the child's real own turn, announced by task_started.
        r#"{"timestamp":"2026-05-05T21:52:20.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"019e5c03-1e99-7000-8000-000000000002"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.100Z","type":"turn_context","payload":{"turn_id":"019e5c03-1e99-7000-8000-000000000002","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":520,"output_tokens":52,"total_tokens":572},"last_token_usage":{"input_tokens":20,"output_tokens":2,"total_tokens":22}}}}"#,
        "\n"
    ));

    let child_messages = parse_codex_file(child.path());

    assert_eq!(child_messages.len(), 1);
    assert_eq!(child_messages[0].tokens.input, 20);
    assert_eq!(child_messages[0].tokens.output, 2);
}

#[test]
fn test_nested_child_skips_replayed_legacy_uuid_v4_turn() {
    // Nested Codex child logs can replay an ancestor turn whose legacy UUID
    // v4 id cannot be ordered against the child's UUID v7 session id. Its
    // task_started timestamp still predates the child, so it must not open
    // the gate or count the inherited token snapshot.
    let child = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:52:10.197Z","type":"session_meta","payload":{"id":"019e5c03-1f5d-7000-8000-000000000001","forked_from_id":"019e5c03-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019e5c03-0000-7000-8000-000000000001","depth":2}}},"thread_source":"subagent","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.197Z","type":"session_meta","payload":{"id":"019e5c03-0000-7000-8000-000000000001","forked_from_id":"019e5b00-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019e5b00-0000-7000-8000-000000000001","depth":1}}},"thread_source":"subagent","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.197Z","type":"session_meta","payload":{"id":"019e5b00-0000-7000-8000-000000000001","source":"cli","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.197Z","type":"event_msg","payload":{"type":"task_started","turn_id":"81d2f55b-894b-4d67-b75b-436ead477f65","started_at":1778017800}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.197Z","type":"turn_context","payload":{"turn_id":"81d2f55b-894b-4d67-b75b-436ead477f65","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.198Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.610Z","type":"event_msg","payload":{"type":"task_started","turn_id":"019e5c03-2100-7000-8000-000000000001","started_at":1779660169}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.611Z","type":"turn_context","payload":{"turn_id":"019e5c03-2100-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.612Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":320,"output_tokens":32,"total_tokens":352},"last_token_usage":{"input_tokens":20,"output_tokens":2,"total_tokens":22}}}}"#,
        "\n"
    ));

    let child_messages = parse_codex_file(child.path());

    assert_eq!(child_messages.len(), 1);
    assert_eq!(child_messages[0].tokens.input, 20);
    assert_eq!(child_messages[0].tokens.output, 2);
}

#[test]
fn test_forked_child_legacy_turn_pins_seconds_unit_contract() {
    // Pins the `started_at` unit contract used by
    // `forked_child_task_starts_own_session`: it compares against the
    // child's fork second (`started_at >= child_started_at_ms / 1000`),
    // so a legacy replayed turn timestamped exactly one second before
    // the child's fork second must stay rejected, while one landing on
    // that same second must be admitted. The child's UUID v7 id here
    // (`018bcfe5-6800-...`) encodes ms=1700000000000, i.e.
    // floor(ms/1000) == 1700000000, a round multiple of 1000 so there is
    // no ambiguity from the ms->s truncation.
    let child = create_test_file(concat!(
        r#"{"timestamp":"2023-11-14T22:13:20.000Z","type":"session_meta","payload":{"id":"018bcfe5-6800-7000-8000-000000000001","forked_from_id":"018bcfe5-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"018bcfe5-0000-7000-8000-000000000001","depth":2}}},"thread_source":"subagent","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2023-11-14T22:13:20.000Z","type":"session_meta","payload":{"id":"018bcfe5-0000-7000-8000-000000000001","forked_from_id":"018bcfe4-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"018bcfe4-0000-7000-8000-000000000001","depth":1}}},"thread_source":"subagent","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2023-11-14T22:13:20.000Z","type":"session_meta","payload":{"id":"018bcfe4-0000-7000-8000-000000000001","source":"cli","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        // legacy turn one second BEFORE the child's fork second
        // (1700000000) -- must NOT open the gate or count its token
        // snapshot.
        r#"{"timestamp":"2023-11-14T22:13:19.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"71d2f55b-894b-4d67-b75b-436ead477f65","started_at":1699999999}}"#,
        "\n",
        r#"{"timestamp":"2023-11-14T22:13:19.000Z","type":"turn_context","payload":{"turn_id":"71d2f55b-894b-4d67-b75b-436ead477f65","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2023-11-14T22:13:19.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        // legacy turn exactly AT the child's fork second -- must be
        // admitted.
        r#"{"timestamp":"2023-11-14T22:13:20.100Z","type":"event_msg","payload":{"type":"task_started","turn_id":"82d2f55b-894b-4d67-b75b-436ead477f66","started_at":1700000000}}"#,
        "\n",
        r#"{"timestamp":"2023-11-14T22:13:20.200Z","type":"turn_context","payload":{"turn_id":"82d2f55b-894b-4d67-b75b-436ead477f66","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2023-11-14T22:13:20.300Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":320,"output_tokens":32,"total_tokens":352},"last_token_usage":{"input_tokens":20,"output_tokens":2,"total_tokens":22}}}}"#,
        "\n"
    ));

    let child_messages = parse_codex_file(child.path());

    assert_eq!(child_messages.len(), 1);
    assert_eq!(child_messages[0].tokens.input, 20);
    assert_eq!(child_messages[0].tokens.output, 2);
}

#[test]
fn test_forked_child_task_started_non_numeric_started_at_does_not_fail_parsing() {
    // A `task_started` event with a non-integer `started_at` (e.g. a
    // string, from a malformed or unexpected log) must not fail
    // deserialization of the whole JSONL line -- it should decode with
    // `started_at: None`, which keeps the replay gate closed (same as a
    // missing timestamp) and still allows the rest of the file,
    // including a valid subsequent `task_started`, to parse normally.
    let child = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5c03-1e99-7000-8000-000000000001","forked_from_id":"019e5b00-0000-7000-8000-000000000001","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019e5b00-0000-7000-8000-000000000001","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.000Z","type":"session_meta","payload":{"id":"019e5b00-0000-7000-8000-000000000001","source":"vscode","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.100Z","type":"turn_context","payload":{"turn_id":"019e5b00-0001-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:10.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        // legacy task_started with a malformed (non-numeric) started_at
        // -- must decode with started_at: None (not fail the whole
        // entry), so the gate stays closed rather than opening on a
        // wrong-typed value.
        r#"{"timestamp":"2026-05-05T21:52:15.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"71d2f55b-894b-4d67-b75b-436ead477f65","started_at":"not-a-number"}}"#,
        "\n",
        // the child's real own turn, announced by a well-formed
        // task_started -- proves the malformed line above didn't corrupt
        // parser state or halt parsing of the rest of the file.
        r#"{"timestamp":"2026-05-05T21:52:20.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"019e5c03-6425-7000-8000-000000000001"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.100Z","type":"turn_context","payload":{"turn_id":"019e5c03-6425-7000-8000-000000000001","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:52:20.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":320,"output_tokens":32,"total_tokens":352},"last_token_usage":{"input_tokens":20,"output_tokens":2,"total_tokens":22}}}}"#,
        "\n"
    ));

    let parsed = parse_codex_file_incremental(child.path(), 0, CodexParseState::default());

    // The malformed line did not abort file-level parsing.
    assert!(parsed.parse_succeeded);
    // The malformed task_started did not open the gate; only the
    // well-formed one that follows (admitted via the UUID v7 ordering
    // path) did.
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].tokens.input, 20);
    assert_eq!(parsed.messages[0].tokens.output, 2);
}

#[test]
fn test_forked_child_incremental_state_skips_inherited_prefix() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:51:57.991Z","type":"session_meta","payload":{"id":"child-session","forked_from_id":"parent-session","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-session","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.992Z","type":"session_meta","payload":{"id":"parent-session","source":"interactive","model_provider":"azure","agent_nickname":"parent","cwd":"/repo-parent"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.994Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":116000,"cached_input_tokens":114000,"output_tokens":1000,"total_tokens":117000},"last_token_usage":{"input_tokens":73000,"cached_input_tokens":72000,"output_tokens":500,"total_tokens":73500}}}}"#,
        "\n"
    ));
    let prefix_size = file.as_file().metadata().unwrap().len();
    let prefix = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());

    assert!(prefix.parse_succeeded);
    assert!(!prefix.unresolved_model_events);
    assert!(prefix.messages.is_empty());

    let appended = concat!(
        r#"{"timestamp":"2026-05-05T21:51:58.947Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.948Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":116000,"cached_input_tokens":114000,"output_tokens":1000,"total_tokens":117000},"last_token_usage":{"input_tokens":73000,"cached_input_tokens":72000,"output_tokens":500,"total_tokens":73500}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:59.253Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":117500,"cached_input_tokens":115000,"output_tokens":1200,"reasoning_output_tokens":50,"total_tokens":118700},"last_token_usage":{"input_tokens":1500,"cached_input_tokens":1000,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":1700}}}}"#,
        "\n"
    );
    let mut reopened = file.reopen().unwrap();
    reopened.seek(SeekFrom::End(0)).unwrap();
    reopened.write_all(appended.as_bytes()).unwrap();
    reopened.flush().unwrap();

    let incremental = parse_codex_file_incremental(file.path(), prefix_size, prefix.state.clone());
    let full = parse_codex_file(file.path());

    assert_eq!(incremental.messages, full);
    assert_eq!(incremental.messages.len(), 1);
    assert_eq!(incremental.messages[0].tokens.input, 500);
    assert_eq!(incremental.messages[0].tokens.cache_read, 1000);
    assert_eq!(incremental.messages[0].tokens.output, 150);
    assert_eq!(incremental.messages[0].tokens.reasoning, 50);
}
