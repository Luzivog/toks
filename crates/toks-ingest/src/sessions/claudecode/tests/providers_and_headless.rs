use super::*;

#[test]
fn test_anthropic_prefixed_claude_model_is_canonicalized() {
    let content = r#"{"type":"assistant","timestamp":"2026-05-27T10:00:00.000Z","message":{"model":"anthropic/claude-4-6-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
}

#[test]
fn test_multi_provider_models_infer_provider_from_model() {
    let content = r#"{"type":"assistant","timestamp":"2026-02-18T10:00:00.000Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":10}}}
{"type":"assistant","timestamp":"2026-02-18T10:00:01.000Z","message":{"model":"gpt-5.3-codex","usage":{"input_tokens":200,"output_tokens":20}}}
{"type":"assistant","timestamp":"2026-02-18T10:00:02.000Z","message":{"model":"gemini-3-flash-preview","usage":{"input_tokens":300,"output_tokens":30}}}
{"type":"assistant","timestamp":"2026-02-18T10:00:03.000Z","message":{"model":"MiniMax-M2.1","usage":{"input_tokens":400,"output_tokens":40}}}
{"type":"assistant","timestamp":"2026-02-18T10:00:04.000Z","message":{"model":"<synthetic>","usage":{"input_tokens":500,"output_tokens":50}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[1].provider_id, "openai");
    assert_eq!(messages[2].provider_id, "google");
    assert_eq!(messages[3].provider_id, "minimax");
    assert!(!messages
        .iter()
        .any(|message| message.model_id == "<synthetic>"));
}

#[test]
fn test_multi_provider_models_prefer_specific_model_over_default_anthropic_hint() {
    let content = r#"{"type":"assistant","provider":"anthropic","timestamp":"2026-02-18T10:00:00.000Z","message":{"model":"gpt-5.3-codex","usage":{"input_tokens":200,"output_tokens":20}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.3-codex");
    assert_eq!(messages[0].provider_id, "openai");
}

#[test]
fn test_multi_provider_models_preserve_reseller_provider_hint() {
    let content = r#"{"type":"assistant","timestamp":"2026-02-18T10:00:00.000Z","message":{"provider":"openrouter/anthropic","model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":10}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-opus-4-6");
    assert_eq!(messages[0].provider_id, "openrouter");
}

#[test]
fn test_headless_json_output() {
    let content = r#"{"type":"message","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":120,"output_tokens":60,"cache_read_input_tokens":10}}}"#;
    let file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    std::fs::write(file.path(), content).unwrap();

    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-3-5-sonnet");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 60);
    assert_eq!(messages[0].tokens.cache_read, 10);
}

#[test]
fn test_headless_json_output_infers_subprovider() {
    let content = r#"{"type":"message","message":{"model":"gpt-5.3-codex","usage":{"input_tokens":120,"output_tokens":60,"cache_read_input_tokens":10}}}"#;
    let file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    std::fs::write(file.path(), content).unwrap();

    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.3-codex");
    assert_eq!(messages[0].provider_id, "openai");
}

#[test]
fn test_headless_json_output_keeps_workspace_metadata() {
    let content = r#"{"type":"message","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":120,"output_tokens":60,"cache_read_input_tokens":10}}}"#;
    let (_dir, path) = create_project_file(content, "myproject", "session.json");

    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key.as_deref(), Some("myproject"));
    assert_eq!(messages[0].workspace_label.as_deref(), Some("myproject"));
}

#[test]
fn test_headless_stream_output() {
    let content = r#"{"type":"message_start","timestamp":"2025-01-01T00:00:00Z","message":{"id":"msg_1","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"cache_read_input_tokens":20,"cache_creation_input_tokens":5}}}
{"type":"message_delta","usage":{"output_tokens":80}}
{"type":"message_stop"}"#;
    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-3-5-sonnet");
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.output, 80);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.cache_write, 5);
}

#[test]
fn test_headless_stream_output_infers_subprovider() {
    let content = r#"{"type":"message_start","timestamp":"2026-02-18T10:00:00Z","message":{"id":"msg_1","model":"gemini-3-pro-preview","usage":{"input_tokens":200,"cache_read_input_tokens":20}}}
{"type":"message_delta","usage":{"output_tokens":80}}
{"type":"message_stop"}"#;
    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-3-pro-preview");
    assert_eq!(messages[0].provider_id, "google");
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.output, 80);
}

#[test]
fn test_headless_synthetic_stream_deltas_do_not_leak_into_next_response() {
    let content = r#"{"type":"message_start","timestamp":"2026-06-24T01:00:00Z","message":{"model":"<synthetic>","usage":{"input_tokens":0}}}
{"type":"message_delta","usage":{"output_tokens":999,"cache_read_input_tokens":888}}
{"type":"message_stop"}
{"type":"message_start","timestamp":"2026-06-24T01:00:02Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_read_input_tokens":2}}}
{"type":"message_delta","usage":{"output_tokens":3}}
{"type":"message_stop"}"#;
    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 3);
    assert_eq!(messages[0].tokens.cache_read, 2);
}
#[test]
fn test_truncated_headless_synthetic_stream_does_not_leak_into_next_response() {
    let content = r#"{"type":"message_start","timestamp":"2026-06-24T01:00:00Z","message":{"model":"<synthetic>","usage":{"input_tokens":0}}}
{"type":"message_delta","usage":{"output_tokens":999,"cache_read_input_tokens":888}}
{"type":"message_start","timestamp":"2026-06-24T01:00:02Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_read_input_tokens":2}}}
{"type":"message_delta","usage":{"output_tokens":3}}
{"type":"message_stop"}"#;
    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 3);
    assert_eq!(messages[0].tokens.cache_read, 2);
}
