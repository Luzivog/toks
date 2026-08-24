use super::*;

#[test]
fn test_parse_gemini_structure() {
    let json = r#"{
        "sessionId": "ses_123",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:00:00Z",
                "type": "user"
            },
            {
                "id": "msg_2",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-2.0-flash",
                "tokens": {
                    "input": 10,
                    "output": 20,
                    "cached": 5,
                    "thoughts": 0,
                    "tool": 0,
                    "total": 35
                }
            }
        ]
    }"#;

    let mut bytes = json.as_bytes().to_vec();
    let session: GeminiSession = simd_json::from_slice(&mut bytes).unwrap();

    assert_eq!(session.messages.len(), 2);
    assert_eq!(
        session.messages[1].model,
        Some("gemini-2.0-flash".to_string())
    );
}

#[test]
fn test_parse_gemini_with_array_content() {
    let json = r#"{
        "sessionId": "ses_123",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:00:00Z",
                "type": "user",
                "content": [{"text": "Hello"}]
            },
            {
                "id": "msg_2",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "content": "Hi there!",
                "model": "gemini-2.0-flash",
                "tokens": {
                    "input": 10,
                    "output": 20
                }
            }
        ]
    }"#;

    // Create a path that matches the legacy prefix so it passes the 'is_in_chats' filter
    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.0-flash");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 20);
}

#[test]
fn test_parse_gemini_session_normalizes_cached_input() {
    let json = r#"{
        "sessionId": "ses_123",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_2",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-2.0-flash",
                "tokens": {
                    "input": 15,
                    "output": 20,
                    "cached": 5,
                    "thoughts": 2,
                    "total": 37
                }
            }
        ]
    }"#;

    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 2);
    assert_eq!(messages[0].tokens.total(), 37);
}

#[test]
fn test_parse_gemini_session_preserves_already_net_input_when_total_matches() {
    let json = r#"{
        "sessionId": "ses_123",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_2",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-2.0-flash",
                "tokens": {
                    "input": 10,
                    "output": 20,
                    "cached": 5,
                    "thoughts": 2,
                    "total": 37
                }
            }
        ]
    }"#;

    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 2);
    assert_eq!(messages[0].tokens.total(), 37);
}
