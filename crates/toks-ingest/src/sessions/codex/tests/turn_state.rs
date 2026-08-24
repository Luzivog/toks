use super::*;

#[test]
fn codex_human_turn_matches_only_known_system_tags() {
    // Real human prompts that happen to start with markup must still count.
    assert!(codex_message_is_human_turn(Some(
        "how do I center a <div>?"
    )));
    assert!(codex_message_is_human_turn(Some("<div>hi</div>")));
    assert!(codex_message_is_human_turn(Some("  plain question")));
    // Known system-injected context blocks are not human turns.
    assert!(!codex_message_is_human_turn(Some(
        "<environment_context>cwd=/tmp</environment_context>"
    )));
    assert!(!codex_message_is_human_turn(Some(
        "  <system-reminder>be concise</system-reminder>"
    )));
    assert!(!codex_message_is_human_turn(Some(
        "<user_instructions>do X</user_instructions>"
    )));
    // A missing body is never a human turn.
    assert!(!codex_message_is_human_turn(None));
}

#[test]
fn test_pending_model_messages_do_not_bind_across_unrelated_turns() {
    let file = create_test_file(concat!(
        r#"{"type":"session_meta","payload":{"source":"interactive","model_provider":"openai"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:02Z","type":"assistant_message"}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:04Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15,"cached_input_tokens":3,"output_tokens":5},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2}}}}"#,
        "\n"
    ));

    let parsed = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());

    assert!(parsed.parse_succeeded);
    assert!(parsed.unresolved_model_events);
    assert_eq!(parsed.messages.len(), 2);
    assert_eq!(parsed.messages[0].model_id, "unknown");
    assert_eq!(parsed.messages[1].model_id, "gpt-5.5");
}

#[test]
fn test_token_count_ignores_empty_info_model_until_later_valid_model() {
    let file = create_test_file(concat!(
        r#"{"type":"session_meta","payload":{"source":"interactive","model_provider":"openai"}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"","model_name":"","total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        "\n",
        r#"{"timestamp":"2026-04-27T10:00:04Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
        "\n"
    ));

    let parsed = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());

    assert!(parsed.parse_succeeded);
    assert!(!parsed.unresolved_model_events);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].model_id, "gpt-5.5");
}

#[test]
fn test_user_message_marks_next_token_count_as_turn_start() {
    let content = [
            r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"continue please"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
            r#"{"timestamp":"2026-01-01T00:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":4,"output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        ]
        .join("\n");
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert!(
        messages[0].is_turn_start,
        "first reply after a human user_message is a turn start"
    );
    assert!(
        !messages[1].is_turn_start,
        "a later reply with no new user_message is not a turn start"
    );
}

#[test]
fn test_xml_user_message_does_not_mark_turn_start() {
    let content = [
            r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"\n<environment_context>\n  <cwd>/tmp</cwd>\n</environment_context>"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        ]
        .join("\n");
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert!(
        !messages[0].is_turn_start,
        "a system-injected <...> message is not a human turn"
    );
}

#[test]
fn test_exec_user_message_still_marks_turn_start() {
    // A `codex exec` one-shot is headless but still carries a real human
    // prompt, so it counts as exactly one turn (verified against a real
    // `codex exec` session: 1 user_message -> turn_count 1).
    let content = [
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"exec"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"hello"}}"#,
            // A real `codex exec` interleaves an agent_message between the user
            // prompt and the token_count; the deferred turn flag must survive it.
            r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"agent_message","message":"hi"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        ]
        .join("\n");
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert!(
        messages[0].is_turn_start,
        "an exec one-shot with a human prompt counts as one turn"
    );
    assert_eq!(messages[0].agent.as_deref(), Some("headless"));
}

#[test]
fn test_incremental_parse_preserves_pending_turn_start() {
    let content = [
            r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#,
            r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"hello"}}"#,
            "",
        ]
        .join("\n");
    let file = create_test_file(&content);
    let initial_size = file.as_file().metadata().unwrap().len();

    let initial = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());
    assert!(
        initial.messages.is_empty(),
        "no token_count yet, so no message"
    );
    assert!(
        initial.state.pending_turn_start,
        "a pending turn survives a chunk that ends before the token_count"
    );

    let appended = format!(
        "{}\n",
        r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#
    );
    let mut reopened = file.reopen().unwrap();
    reopened.seek(SeekFrom::End(0)).unwrap();
    reopened.write_all(appended.as_bytes()).unwrap();
    reopened.flush().unwrap();

    let incremental =
        parse_codex_file_incremental(file.path(), initial_size, initial.state.clone());

    assert_eq!(incremental.messages.len(), 1);
    assert!(
        incremental.messages[0].is_turn_start,
        "the deferred turn applies to the message parsed in the next chunk"
    );
    assert!(
        !incremental.state.pending_turn_start,
        "the pending flag is consumed once applied"
    );
}
