use super::*;

#[test]
fn parses_jcode_token_usage_messages() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "id":"session_test",
  "provider_key":"cliproxyapi",
  "model":"claude-sonnet-4",
  "working_dir":"/Users/alice/project",
  "messages":[
{"id":"user_1","role":"user","timestamp":"2026-06-16T12:00:00Z","content":[]},
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":1200,"output_tokens":300,"cache_read_input_tokens":800,"cache_creation_input_tokens":50,"reasoning_output_tokens":25},"tool_duration_ms":1234}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_jcode_file(file.path());
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "jcode");
    assert_eq!(message.session_id, "session_test");
    assert_eq!(message.model_id, "claude-sonnet-4");
    assert_eq!(message.provider_id, "cliproxyapi");
    assert_eq!(message.tokens.input, 1200);
    assert_eq!(message.tokens.cache_read, 800);
    assert_eq!(message.tokens.cache_write, 50);
    assert_eq!(message.tokens.output, 300);
    assert_eq!(message.tokens.reasoning, 25);
    assert_eq!(message.duration_ms, Some(1234));
    assert!(message.is_turn_start);
    assert_eq!(message.workspace_label.as_deref(), Some("project"));
}

#[test]
fn subtracts_subset_cache_reads_from_openai_input_tokens() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "id":"session_openai_cache",
  "provider_key":"openai",
  "model":"gpt-5.6-sol",
  "messages":[
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":19347,"output_tokens":71,"cache_read_input_tokens":15872}}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_jcode_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 3_475);
    assert_eq!(messages[0].tokens.cache_read, 15_872);
    assert_eq!(messages[0].tokens.output, 71);
}

#[test]
fn preserves_split_cache_reads_for_anthropic_input_tokens() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "id":"session_anthropic_cache",
  "provider_key":"anthropic-api-key",
  "model":"claude-sonnet-4-5",
  "messages":[
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":20000,"output_tokens":71,"cache_read_input_tokens":15872,"cache_creation_input_tokens":0}}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_jcode_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 20_000);
    assert_eq!(messages[0].tokens.cache_read, 15_872);
    assert_eq!(messages[0].tokens.output, 71);
}

#[test]
fn subtracts_openrouter_cache_for_routed_claude_models() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "id":"session_openrouter_claude",
  "provider_key":"openrouter",
  "model":"anthropic/claude-sonnet-4",
  "messages":[
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":1000,"output_tokens":71,"cache_read_input_tokens":800}}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_jcode_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.cache_read, 800);
}

#[test]
fn marks_only_first_assistant_after_user_as_turn_start() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "id":"session_turns",
  "messages":[
{"id":"user_1","role":"user","timestamp":"2026-06-16T12:00:00Z"},
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}},
{"id":"assistant_2","role":"assistant","timestamp":"2026-06-16T12:00:02Z","token_usage":{"input_tokens":50,"output_tokens":5}},
{"id":"user_2","role":"user","timestamp":"2026-06-16T12:00:03Z"},
{"id":"assistant_3","role":"assistant","timestamp":"2026-06-16T12:00:04Z","token_usage":{"input_tokens":25,"output_tokens":2}}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_jcode_file(file.path());
    assert_eq!(messages.len(), 3);
    assert!(messages[0].is_turn_start);
    assert!(!messages[1].is_turn_start);
    assert!(messages[2].is_turn_start);
}

#[test]
fn one_malformed_token_usage_does_not_drop_the_whole_session() {
    // A single malformed message (token_usage as a string) must not nuke
    // every other valid message in the snapshot (and its journal).
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_good","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}},
{"id":"assistant_bad","role":"assistant","timestamp":"2026-06-16T12:00:02Z","token_usage":"corrupt"}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"append_messages":[{"id":"assistant_journal","role":"assistant","timestamp":"2026-06-16T12:00:03Z","token_usage":{"input_tokens":200,"output_tokens":20}}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(
        messages.len(),
        2,
        "valid snapshot + journal messages must survive one malformed sibling"
    );
    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|m| m.tokens.output).sum();
    assert_eq!(total_input, 300);
    assert_eq!(total_output, 30);
}
