use super::*;

#[test]
fn is_human_turn_counts_html_user_prompt() {
    let line = r#"{"type":"user","message":{"content":"<div>hello</div>"}}"#;
    assert!(is_human_turn(line));
}

#[test]
fn is_human_turn_skips_internal_tool_tags() {
    for tag in CLAUDECODE_INTERNAL_USER_TAGS {
        let line = format!(r#"{{"type":"user","message":{{"content":"{tag}some output</...>"}}}}"#);
        assert!(
            !is_human_turn(&line),
            "expected tag {tag} to be filtered as non-human"
        );
    }
}

#[test]
fn is_human_turn_skips_array_content() {
    let line = r#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#;
    assert!(!is_human_turn(line));
}

#[test]
fn test_user_messages_ignored() {
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Hello"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1, "User messages should be ignored");
    assert_eq!(messages[0].tokens.input, 100);
}

#[test]
fn test_turn_start_detection() {
    // Simulate: user asks → assistant responds → tool_result (as user) → assistant responds
    //         → real user asks again → assistant responds
    // Expected: 2 turns (tool_result should NOT count as a turn)
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Hello"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"user","timestamp":"2024-12-01T10:00:02.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"tu_001","content":"file contents here"}]}}
{"type":"assistant","timestamp":"2024-12-01T10:00:03.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":80}}}
{"type":"user","timestamp":"2024-12-01T10:00:04.000Z","message":{"content":"Thanks, now do X"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:05.000Z","requestId":"req_003","message":{"id":"msg_003","model":"claude-3-5-sonnet","usage":{"input_tokens":300,"output_tokens":120}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        3,
        "Should include 3 assistant messages; tool_result without explicit tokens is not counted"
    );
    let assistant_messages: Vec<_> = messages
        .iter()
        .filter(|message| message.tokens.output > 0)
        .collect();
    assert_eq!(
        assistant_messages.len(),
        3,
        "Should have 3 assistant usage messages"
    );

    // First assistant after first human user → turn start
    assert!(
        assistant_messages[0].is_turn_start,
        "First response should be turn start"
    );
    // Assistant after tool_result → NOT a new turn
    assert!(
        !assistant_messages[1].is_turn_start,
        "Response after tool_result should NOT be turn start"
    );
    // First assistant after second human user → turn start
    assert!(
        assistant_messages[2].is_turn_start,
        "Response after real user input should be turn start"
    );

    let turn_count: usize = messages.iter().filter(|m| m.is_turn_start).count();
    assert_eq!(turn_count, 2, "Should detect 2 turns");
}

#[test]
fn test_turn_start_ignores_system_messages() {
    // XML-tagged content like <local-command-stdout> should not count as turns
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Do something"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"user","timestamp":"2024-12-01T10:00:02.000Z","message":{"content":"<local-command-stdout>ok</local-command-stdout>"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:03.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":80}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 2);
    assert!(
        messages[0].is_turn_start,
        "First response after human input is a turn"
    );
    assert!(
        !messages[1].is_turn_start,
        "Response after local-command should NOT be a turn"
    );

    let turn_count: usize = messages.iter().filter(|m| m.is_turn_start).count();
    assert_eq!(turn_count, 1);
}

#[test]
fn test_turn_start_without_user_message() {
    // No user message → no turn starts (e.g. headless or partial log)
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":100}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 2);
    assert!(!messages[0].is_turn_start);
    assert!(!messages[1].is_turn_start);
}
