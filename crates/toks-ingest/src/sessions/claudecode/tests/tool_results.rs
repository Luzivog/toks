use super::*;

#[test]
fn test_opus_4_7_usage_is_parsed_when_usage_metadata_exists() {
    let content = r#"{"type":"assistant","timestamp":"2026-04-16T10:00:00.000Z","requestId":"req_opus47","message":{"id":"msg_opus47","model":"claude-opus-4-7","usage":{"input_tokens":321,"output_tokens":654,"cache_read_input_tokens":987,"cache_creation_input_tokens":111}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-opus-4-7");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].tokens.input, 321);
    assert_eq!(messages[0].tokens.output, 654);
    assert_eq!(messages[0].tokens.cache_read, 987);
    assert_eq!(messages[0].tokens.cache_write, 111);
}

#[test]
fn test_tool_result_without_explicit_tokens_is_not_char_estimated() {
    // tokscope#1011: Claude Code never writes token metadata on tool_result
    // blocks. The next assistant turn's API usage already includes that
    // content, so ceil(chars/4) would double-count.
    let content = r#"{"type":"user","timestamp":"2026-05-27T10:00:00.000Z","message":{"model":"anthropic/claude-4-6-sonnet","content":[{"type":"tool_result","tool_use_id":"toolu_input","tool_output":{"output":"abcdefghijklmnop"}}]}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert!(
        messages.is_empty(),
        "tool_result rows without explicit token metadata must not be char-estimated"
    );
}

/// History retention across an in-place transcript rewrite is only sound
/// for keys that identify a message by content across files. This pins
/// which of the parser's own key shapes qualify, so a future key change
/// trips here rather than silently making a retained copy double count.
#[test]
fn test_only_content_derived_dedup_keys_are_globally_stable() {
    let tool_result = tool_result_dedup_key("claude", "0f1e2d3c-session", "tool_result:toolu_1");
    assert!(
        !dedup_key_is_globally_stable(&tool_result),
        "tool-result keys embed the transcript file stem: {tool_result}"
    );

    // The two shapes `parse_claude_file` mints for assistant turns.
    assert!(dedup_key_is_globally_stable("msg_01ABC:req_01XYZ"));
    assert!(dedup_key_is_globally_stable("message:msg_01ABC"));
}

#[test]
fn test_cc_mirror_tool_result_keeps_variant_client_and_provider() {
    let content = r#"{"type":"user","timestamp":"2026-05-27T10:00:00.000Z","message":{"model":"sonnet","content":[{"type":"tool_result","tool_use_id":"toolu_cc_mirror","tool_output":{"input_tokens":7,"output":"tool output"}}]}}"#;

    let (_temp_dir, path) =
        create_cc_mirror_project_file(content, "zai-worker", "zai", "project-one", "session.jsonl");
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "cc-mirror/zai-worker");
    assert_eq!(messages[0].provider_id, "zai");
    assert_eq!(messages[0].model_id, "sonnet");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].message_count, 0);
}

#[test]
fn test_tool_result_duplicate_uses_max_input_tokens() {
    let content = r#"{"type":"tool_result","timestamp":"2026-05-27T10:00:00.000Z","model":"anthropic/claude-4-6-sonnet","tool_result":{"tool_use_id":"toolu_stream","tool_output":{"output":"abcdefghijklmnop","input_tokens":4}}}
{"type":"tool_result","timestamp":"2026-05-27T10:00:00.100Z","model":"anthropic/claude-4-6-sonnet","tool_result":{"tool_use_id":"toolu_stream","tool_output":{"output":"abcdefghijklmnopqrstuvwxyzabcd","input_tokens":8}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].tokens.input, 8);
    assert_eq!(messages[0].timestamp, 1_779_876_000_100);
}

#[test]
fn test_tool_result_repeated_in_same_record_is_not_counted_twice() {
    let content = r#"{"type":"tool_result","timestamp":"2026-05-27T10:00:00.000Z","model":"anthropic/claude-4-6-sonnet","tool_result":{"tool_use_id":"toolu_same","tool_output":{"output":"abcdefghijklmnop","input_tokens":4}},"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_same","tool_output":{"output":"abcdefghijklmnop","input_tokens":4}}]}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 4);
}

#[test]
fn test_tool_result_prefers_input_token_metadata_over_char_estimate() {
    let content = r#"{"type":"user","timestamp":"2026-05-27T10:00:00.000Z","message":{"model":"claude-sonnet-4-6","content":[{"type":"tool_result","tool_use_id":"toolu_metadata","tool_output":{"output":"abcdefghijklmnopqrstuvwxyzabcd","input_tokens":3}}]}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 3);
}

#[test]
fn test_synthetic_notice_does_not_seed_an_unmodelled_tool_result() {
    let content = r#"{"type":"user","timestamp":"2026-06-24T01:00:00.000Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}
{"type":"assistant","timestamp":"2026-06-24T01:00:01.000Z","isApiErrorMessage":true,"error":"unknown","message":{"id":"m1","role":"assistant","model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"API Error"}]}}
{"type":"user","timestamp":"2026-06-24T01:00:02.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"XXXXXXXXXXXXXXXX"}]}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_synthetic_notice_does_not_hide_an_explicitly_modelled_tool_result() {
    let content = r#"{"type":"assistant","timestamp":"2026-06-24T01:00:01.000Z","message":{"model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0}}}
{"type":"user","timestamp":"2026-06-24T01:00:02.000Z","message":{"model":"claude-sonnet-4-6","content":[{"tool_use_id":"toolu_1","type":"tool_result","tool_output":{"output":"XXXXXXXXXXXXXXXX","input_tokens":4}}]}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    // Explicit tool-result token metadata is honored even after a
    // synthetic notice; the char fallback stays off for this client (#1011).
    assert_eq!(messages[0].tokens.input, 4);
}

#[test]
fn test_inherited_synthetic_model_does_not_price_a_tool_result() {
    // A `<synthetic>` notice that is not immediately followed by the tool
    // result in the same transcript still leaves the placeholder as the
    // inherited carrier model. The usage must be dropped rather than
    // emitted as `unknown/<synthetic>`: submission cannot price that model,
    // so it either excludes the row or fails outright depending on pricing
    // coverage. See the guard in `extract_claude_tool_result_message`.
    let context = ClaudeToolResultContext {
        entry: &ClaudeEntry {
            entry_type: "user".to_string(),
            timestamp: Some("2026-05-30T01:00:00.000Z".to_string()),
            message: None,
            request_id: None,
            is_sidechain: false,
            agent_id: None,
            session_id: None,
            provider_id: None,
        },
        last_model: Some("<synthetic>"),
        last_provider_hint: None,
        client_id: "claude",
        default_provider_hint: None,
        session_id: "session",
        fallback_timestamp: 1_782_259_200_000,
        workspace_key: None,
        workspace_label: None,
        sidechain_agent: None,
        suppress_unattributed: false,
        allow_char_estimate: true,
    };
    let raw = r#"{"type":"user","timestamp":"2026-05-30T01:00:00.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"XXXXXXXXXXXXXXXX"}]}}"#;

    assert!(extract_claude_tool_result_message(raw.as_bytes(), context).is_none());
}

#[test]
fn test_synthetic_notice_does_not_emit_a_provider_only_tool_result() {
    let content = r#"{"type":"assistant","timestamp":"2026-06-24T01:00:01.000Z","message":{"model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0}}}
{"type":"user","timestamp":"2026-06-24T01:00:02.000Z","provider":"openrouter","message":{"content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"XXXXXXXXXXXXXXXX"}]}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_api_reported_input_not_inflated_by_tool_result_char_estimate() {
    // Minimal repro from tokscope#1011: assistant usage.input_tokens=7 plus a
    // 21-char tool_result. Before the fix, reported input was 13
    // (7 + ceil(21/4)=6). After, only the API figure remains.
    let content = concat!(
        r#"{"type":"assistant","timestamp":"2026-05-27T10:00:00.000Z","message":{"id":"msg_api","model":"claude-sonnet-4-6","content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"/tmp/x"}}],"usage":{"input_tokens":7,"output_tokens":1}}}"#,
        "\n",
        r#"{"type":"user","timestamp":"2026-05-27T10:00:01.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"123456789012345678901"}]}}"#,
        "\n",
    );
    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    assert_eq!(
        total_input, 7,
        "tool_result char estimate must not stack on API input_tokens; got {messages:#?}"
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.output, 1);
}

#[test]
fn test_assistant_usage_with_tool_use_is_not_estimated_from_prompt_text() {
    let content = r#"{"type":"assistant","timestamp":"2026-05-27T10:00:00.000Z","message":{"id":"msg_tool_use","model":"claude-sonnet-4-6","content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/tmp/large.txt"}}],"usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
}
