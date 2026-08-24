use super::*;

#[test]
fn test_forked_child_ignores_inherited_records_before_turn_context() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:51:57.991Z","type":"session_meta","payload":{"id":"child-session","forked_from_id":"parent-session","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-session","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.992Z","type":"session_meta","payload":{"id":"parent-session","source":"interactive","model_provider":"azure","agent_nickname":"parent","cwd":"/repo-parent"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.993Z","type":"event_msg","payload":{"type":"user_message","message":"parent prompt copied into child log"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.994Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":116000,"cached_input_tokens":114000,"output_tokens":1000,"total_tokens":117000},"last_token_usage":{"input_tokens":73000,"cached_input_tokens":72000,"output_tokens":500,"total_tokens":73500}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.947Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.948Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":116000,"cached_input_tokens":114000,"output_tokens":1000,"total_tokens":117000},"last_token_usage":{"input_tokens":73000,"cached_input_tokens":72000,"output_tokens":500,"total_tokens":73500}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:59.253Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":117500,"cached_input_tokens":115000,"output_tokens":1200,"reasoning_output_tokens":50,"total_tokens":118700},"last_token_usage":{"input_tokens":1500,"cached_input_tokens":1000,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":1700}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.5");
    assert_eq!(messages[0].provider_id, "openai");
    assert_eq!(messages[0].agent.as_deref(), Some("worker"));
    assert_eq!(messages[0].workspace_key.as_deref(), Some("/repo-child"));
    assert_eq!(messages[0].tokens.input, 500);
    assert_eq!(messages[0].tokens.cache_read, 1000);
    assert_eq!(messages[0].tokens.output, 150);
    assert_eq!(messages[0].tokens.reasoning, 50);
    assert_eq!(messages[0].tokens.total(), 1_700);
}

#[test]
fn test_forked_child_ignores_replayed_parent_rows_after_turn_context() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:51:57.991Z","type":"session_meta","payload":{"id":"child-session","forked_from_id":"parent-session","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-session","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.994Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.947Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.948Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":50,"output_tokens":5,"total_tokens":55},"last_token_usage":{"input_tokens":50,"output_tokens":5,"total_tokens":55}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.949Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":250,"output_tokens":25,"total_tokens":275}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:59.253Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":310,"output_tokens":32,"total_tokens":342},"last_token_usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.5");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 2);
}

#[test]
fn test_forked_child_submit_cap_regression_skips_large_inherited_cache_replays() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:51:57.991Z","type":"session_meta","payload":{"id":"child-session","forked_from_id":"parent-session","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-session","depth":1,"agent_role":"architect"}}},"model_provider":"openai","agent_nickname":"architect","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.994Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200000000,"cached_input_tokens":1180000000,"output_tokens":1000000,"reasoning_output_tokens":100000,"total_tokens":1201100000},"last_token_usage":{"input_tokens":750000000,"cached_input_tokens":740000000,"output_tokens":500000,"reasoning_output_tokens":50000,"total_tokens":750550000}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.947Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.948Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1180000000,"cached_input_tokens":1160000000,"output_tokens":900000,"reasoning_output_tokens":90000,"total_tokens":1180990000},"last_token_usage":{"input_tokens":20000000,"cached_input_tokens":20000000,"output_tokens":0,"reasoning_output_tokens":0,"total_tokens":20000000}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.949Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200000000,"cached_input_tokens":1180000000,"output_tokens":1000000,"reasoning_output_tokens":100000,"total_tokens":1201100000},"last_token_usage":{"input_tokens":20000000,"cached_input_tokens":20000000,"output_tokens":100000,"reasoning_output_tokens":10000,"total_tokens":20110000}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:59.253Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200001500,"cached_input_tokens":1180001000,"output_tokens":1000200,"reasoning_output_tokens":100050,"total_tokens":1201101750},"last_token_usage":{"input_tokens":1500,"cached_input_tokens":1000,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":1750}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.5");
    assert_eq!(messages[0].tokens.input, 500);
    assert_eq!(messages[0].tokens.cache_read, 1000);
    assert_eq!(messages[0].tokens.output, 150);
    assert_eq!(messages[0].tokens.reasoning, 50);
}

#[test]
fn test_forked_child_detects_thread_spawn_source_without_top_level_fork_id() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-05-05T21:51:57.991Z","type":"session_meta","payload":{"id":"child-session","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-session","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:57.994Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330},"last_token_usage":{"input_tokens":300,"output_tokens":30,"total_tokens":330}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.947Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"/repo-child"}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:58.948Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":50,"output_tokens":5,"total_tokens":55},"last_token_usage":{"input_tokens":50,"output_tokens":5,"total_tokens":55}}}}"#,
        "\n",
        r#"{"timestamp":"2026-05-05T21:51:59.253Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":310,"output_tokens":32,"total_tokens":342},"last_token_usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.5");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 2);
}

#[test]
fn test_user_forked_child_counts_own_turn_after_parent_replay() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-01-02T03:10:00.000Z","type":"session_meta","payload":{"id":"22222222-2222-7222-8222-222222222222","forked_from_id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.001Z","type":"session_meta","payload":{"id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.100Z","type":"turn_context","payload":{"turn_id":"11111111-3333-7333-8333-333333333333","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100}}}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:30.100Z","type":"turn_context","payload":{"turn_id":"22222222-4444-7444-8444-444444444444","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:30.200Z","type":"session_meta","payload":{"id":"22222222-2222-7222-8222-222222222222","forked_from_id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:31.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1250,"cached_input_tokens":450,"output_tokens":120,"total_tokens":1370},"last_token_usage":{"input_tokens":250,"cached_input_tokens":50,"output_tokens":20,"total_tokens":270}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.output, 20);
}

#[test]
fn test_user_forked_child_same_millisecond_own_turn_counts_without_task_started() {
    // Human (`thread_source:"user"`) fork where the child session_meta and
    // the child's own first turn_context are minted in the SAME millisecond,
    // so both UUID v7 ids share the 48-bit prefix (`22222222-2222`). A user
    // fork never emits a `task_started`, so a same-millisecond gate that
    // requires `task_started` would keep skipping forever and drop the
    // child's own turn (0 messages). The replayed parent turn carries the
    // *parent's* millisecond prefix (`11111111`), so it sorts strictly
    // earlier and is still skipped; only the child's own turn — the one that
    // shares the child's fork millisecond — must end the skip and be counted
    // (200/20 delta from the inherited 1000/100 baseline).
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-01-02T03:10:00.000Z","type":"session_meta","payload":{"id":"22222222-2222-7222-8222-222222222222","forked_from_id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.001Z","type":"session_meta","payload":{"id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.100Z","type":"turn_context","payload":{"turn_id":"11111111-3333-7333-8333-333333333333","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100}}}}"#,
        "\n",
        // child's own turn: turn_id shares the child session's millisecond
        // prefix (`22222222-2222`) — same-millisecond tie with the fork.
        r#"{"timestamp":"2026-01-02T03:10:30.100Z","type":"turn_context","payload":{"turn_id":"22222222-2222-7444-8444-444444444444","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:30.200Z","type":"session_meta","payload":{"id":"22222222-2222-7222-8222-222222222222","forked_from_id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:31.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1250,"cached_input_tokens":450,"output_tokens":120,"total_tokens":1370},"last_token_usage":{"input_tokens":250,"cached_input_tokens":50,"output_tokens":20,"total_tokens":270}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.output, 20);
}

#[test]
fn test_user_fork_replayed_parent_shares_child_ms_ends_skip_early() {
    // Documents (locks) the accepted residual called out at the Equal branch:
    // a human (`thread_source:"user"`) fork resolves a same-millisecond tie on
    // the millisecond prefix alone, because user forks never emit a
    // `task_started` to harden the gate. Here the *replayed parent* turn is
    // (pathologically) minted within the exact same 1ms as the child's fork
    // session_meta, so it shares the *child's* prefix (`22222222-2222`) rather
    // than the parent's. Because the gate cannot distinguish it from the
    // child's own turn, it ends the skip one turn early and counts that
    // replayed parent row (500/50 delta off the 1000/100 baseline) as the
    // child's first turn. This is a sub-millisecond, human-paced coincidence;
    // the test pins the CURRENT behavior so any future change is intentional.
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-01-02T03:10:00.000Z","type":"session_meta","payload":{"id":"22222222-2222-7222-8222-222222222222","forked_from_id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.001Z","type":"session_meta","payload":{"id":"11111111-1111-7111-8111-111111111111","source":"vscode","thread_source":"user","model_provider":"openai","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":100,"total_tokens":1100}}}}"#,
        "\n",
        // replayed parent turn whose turn_id coincidentally shares the child's
        // fork millisecond prefix (`22222222-2222`) — equal-prefix tie. With a
        // user fork (no task_started) this ends the skip here, one turn early.
        r#"{"timestamp":"2026-01-02T03:10:00.200Z","type":"turn_context","payload":{"turn_id":"22222222-2222-7333-8333-333333333333","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:00.300Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1500,"cached_input_tokens":450,"output_tokens":150,"total_tokens":1650},"last_token_usage":{"input_tokens":500,"cached_input_tokens":50,"output_tokens":50,"total_tokens":550}}}}"#,
        "\n",
        // the child's actual own turn (also shares the child's prefix).
        r#"{"timestamp":"2026-01-02T03:10:30.100Z","type":"turn_context","payload":{"turn_id":"22222222-2222-7444-8444-444444444444","model":"gpt-5.5","cwd":"/repo"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-02T03:10:31.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1750,"cached_input_tokens":500,"output_tokens":170,"total_tokens":1920},"last_token_usage":{"input_tokens":250,"cached_input_tokens":50,"output_tokens":20,"total_tokens":270}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    // CURRENT behavior: the skip ends one turn early at the equal-prefix
    // replayed parent turn, so BOTH that row and the child's own turn are
    // counted (two messages) rather than only the child's own turn. The
    // first message is the replayed parent delta (total 1500-1000=500, of
    // which 50 is cache_read, leaving 450 non-cached input + 50 output); the
    // second is the child's own delta (250-50=200 input + 20 output). A
    // future change that hardened this tie would instead yield a single
    // message with the child's 200/20.
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 450);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[1].tokens.input, 200);
    assert_eq!(messages[1].tokens.cache_read, 50);
    assert_eq!(messages[1].tokens.output, 20);
}
