use super::*;

#[test]
fn test_session_meta_exec_marks_headless() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"originator":"codex_exec","source":"exec"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#;
    let content = format!("{}\n{}", line1, line2);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent.as_deref(), Some("headless"));
}

#[test]
fn test_model_info_slug_from_turn_context() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model_info":{"slug":"o3-pro"}}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}", line1, line2);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "o3-pro");
    assert_eq!(messages[0].duration_ms, Some(1000));
}

#[test]
fn test_session_meta_provider_and_agent() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"interactive","model_provider":"azure","agent_nickname":"my-agent"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "azure");
    assert_eq!(messages[0].agent.as_deref(), Some("my-agent"));
}

#[test]
fn test_session_meta_object_source_keeps_provider_agent_and_workspace() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":"fork-session","forked_from_id":"parent-session","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-session","depth":1}}},"model_provider":"openai","agent_nickname":"worker","cwd":"/Users/alice/codex-fork"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        "\n"
    ));

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openai");
    assert_eq!(messages[0].agent.as_deref(), Some("worker"));
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/alice/codex-fork")
    );
    assert!(messages[0].dedup_key.is_some());
}

#[test]
fn test_session_meta_cwd_sets_workspace_metadata() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"interactive","cwd":"/Users/alice/demo-repo"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/alice/demo-repo")
    );
    assert_eq!(messages[0].workspace_label.as_deref(), Some("demo-repo"));
}

#[test]
fn test_inaccessible_cwd_still_parses_token_usage() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"interactive","cwd":"/path/that/does/not/exist/demo-repo"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 8);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 2);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/path/that/does/not/exist/demo-repo")
    );
    assert_eq!(messages[0].workspace_label.as_deref(), Some("demo-repo"));
}

#[test]
fn test_session_meta_empty_cwd_clears_workspace_metadata() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"interactive","cwd":"   "}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key, None);
    assert_eq!(messages[0].workspace_label, None);
    assert_eq!(messages[0].tokens.input, 8);
}

#[test]
fn test_session_meta_malformed_cwd_clears_workspace_metadata() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"interactive","cwd":"file:///Users/alice/demo-repo"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key, None);
    assert_eq!(messages[0].workspace_label, None);
    assert_eq!(messages[0].tokens.input, 8);
}

#[test]
fn test_session_meta_path_like_noncanonical_cwd_normalizes_consistently() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"source":"interactive","cwd":"//server//share///demo-repo/"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("//server/share/demo-repo")
    );
    assert_eq!(messages[0].workspace_label.as_deref(), Some("demo-repo"));
    assert_eq!(messages[0].tokens.input, 8);
}

#[test]
fn test_headless_fallback_uses_session_provider_and_agent() {
    // session_meta sets provider to "azure" and agent to "my-bot",
    // then a line falls through to headless parsing (no structured entry_type)
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"model_provider":"azure","agent_nickname":"my-bot"}}"#;
    let line2 = r#"{"type":"turn.completed","model":"gpt-4o","usage":{"input_tokens":100,"output_tokens":50}}"#;
    let content = format!("{}\n{}", line1, line2);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "azure");
    assert_eq!(messages[0].agent.as_deref(), Some("my-bot"));
}

#[test]
fn test_headless_fallback_defaults_to_openai_without_session_meta() {
    // No session_meta — headless fallback should default to "openai"
    let content = r#"{"type":"turn.completed","model":"gpt-4o-mini","usage":{"input_tokens":120,"cached_input_tokens":20,"output_tokens":30}}"#;
    let file = create_test_file(content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openai");
    assert!(messages[0].agent.is_none());
}

#[test]
fn test_extract_model_skips_empty_slug_falls_through_to_model() {
    // model_info.slug is empty string, but payload.model has a valid value.
    // extract_model should skip the empty slug and return payload.model.
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model_info":{"slug":""},"model":"gpt-4o"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"output_tokens":5}}}}"#;
    let content = format!("{}\n{}", line1, line2);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-4o");
}
