use super::*;

#[test]
fn test_headless_usage_line() {
    let content = r#"{"type":"turn.completed","model":"gpt-4o-mini","usage":{"input_tokens":120,"cached_input_tokens":20,"output_tokens":30}}"#;
    let file = create_test_file(content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-4o-mini");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 30);
    assert_eq!(messages[0].tokens.cache_read, 20);
}

#[test]
fn test_headless_usage_nested_data() {
    let content = r#"{"type":"result","data":{"model_name":"gpt-4o","usage":{"input_tokens":50,"cached_input_tokens":5,"output_tokens":12}}}"#;
    let file = create_test_file(content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-4o");
    assert_eq!(messages[0].tokens.input, 45);
    assert_eq!(messages[0].tokens.output, 12);
    assert_eq!(messages[0].tokens.cache_read, 5);
}

#[test]
fn test_incremental_parse_matches_full_parse_for_appended_lines() {
    let file = create_test_file(concat!(
        r#"{"type":"session_meta","payload":{"source":"chat","model_provider":"openai","agent_nickname":"builder","cwd":"/Users/alice/codex-demo"}}"#,
        "\n",
        r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        "\n"
    ));

    let initial_size = file.as_file().metadata().unwrap().len();
    let initial = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());
    assert_eq!(initial.messages.len(), 1);
    assert_eq!(initial.consumed_offset, initial_size);
    assert_eq!(
        initial.messages[0].workspace_key.as_deref(),
        Some("/Users/alice/codex-demo")
    );
    assert_eq!(
        initial.messages[0].workspace_label.as_deref(),
        Some("codex-demo")
    );

    let appended = concat!(
        r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15,"cached_input_tokens":3,"output_tokens":5},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2}}}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":22,"cached_input_tokens":4,"output_tokens":7},"last_token_usage":{"input_tokens":7,"cached_input_tokens":1,"output_tokens":2}}}}"#,
        "\n"
    );

    let mut reopened = file.reopen().unwrap();
    reopened.seek(SeekFrom::End(0)).unwrap();
    reopened.write_all(appended.as_bytes()).unwrap();
    reopened.flush().unwrap();

    let incremental =
        parse_codex_file_incremental(file.path(), initial_size, initial.state.clone());
    let mut combined = initial.messages.clone();
    combined.extend(incremental.messages);
    assert_eq!(
        incremental.consumed_offset,
        file.as_file().metadata().unwrap().len()
    );

    let full = parse_codex_file(file.path());
    assert_eq!(combined, full);
    assert_eq!(
        combined
            .iter()
            .map(|msg| msg.workspace_key.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("/Users/alice/codex-demo"),
            Some("/Users/alice/codex-demo"),
            Some("/Users/alice/codex-demo")
        ]
    );
}

#[test]
fn test_token_count_before_turn_context_uses_later_model() {
    let file = create_test_file(concat!(
        r#"{"type":"session_meta","payload":{"source":"interactive","model_provider":"openai","agent_nickname":"builder","cwd":"/Users/alice/codex-demo"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15,"cached_input_tokens":3,"output_tokens":5,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":0}}}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:04Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":22,"cached_input_tokens":4,"output_tokens":7,"reasoning_output_tokens":2},"last_token_usage":{"input_tokens":7,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":1}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.model_id.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5.5", "gpt-5.5", "gpt-5.5"]
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.workspace_key.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("/Users/alice/codex-demo"),
            Some("/Users/alice/codex-demo"),
            Some("/Users/alice/codex-demo")
        ]
    );
    assert_eq!(messages[0].tokens.input, 8);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 2);
    assert_eq!(messages[0].tokens.reasoning, 1);
    assert_eq!(messages[1].tokens.input, 4);
    assert_eq!(messages[1].tokens.output, 2);
    assert_eq!(messages[1].tokens.cache_read, 1);
    assert_eq!(messages[1].tokens.reasoning, 0);
    assert_eq!(messages[2].tokens.input, 6);
    assert_eq!(messages[2].tokens.output, 1);
    assert_eq!(messages[2].tokens.cache_read, 1);
    assert_eq!(messages[2].tokens.reasoning, 1);

    let parsed = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());
    assert!(!parsed.unresolved_model_events);
}

#[test]
fn test_token_count_without_model_stays_unknown_but_is_not_cacheable() {
    let file = create_test_file(concat!(
        r#"{"type":"session_meta","payload":{"source":"interactive","model_provider":"openai"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#,
        "\n"
    ));

    let parsed = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());

    assert!(parsed.parse_succeeded);
    assert!(parsed.unresolved_model_events);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].model_id, "unknown");
}

#[test]
fn test_model_only_headless_line_flushes_pending_token_counts() {
    let file = create_test_file(concat!(
        r#"{"type":"session_meta","payload":{"source":"interactive","model_provider":"openai"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#,
        "\n",
        r#"{"model":"gpt-5.5","type":"metadata"}"#,
        "\n"
    ));

    let parsed = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());

    assert!(parsed.parse_succeeded);
    assert!(!parsed.unresolved_model_events);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].model_id, "gpt-5.5");
}
