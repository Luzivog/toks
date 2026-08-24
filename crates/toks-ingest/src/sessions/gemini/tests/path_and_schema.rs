use super::*;

#[test]
fn test_parse_gemini_valid_uuid_path() {
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
                    "output": 20
                }
            }
        ]
    }"#;

    let dir = TempDir::new().unwrap();
    let base = dir.path();
    let chats_dir = base.join(".gemini/tmp/abc123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("uuid-file.json");
    std::fs::write(&file_path, json).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.0-flash");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 20);
}

#[test]
fn test_parse_gemini_reject_nested_chats() {
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
                "content": [{"text": "test"}],
                "model": "gemini-2.0-flash",
                "tokens": {
                    "input": 10,
                    "output": 20
                }
            }
        ]
    }"#;

    let dir = TempDir::new().unwrap();
    let base = dir.path();
    let nested_dir = base.join(".gemini/tmp/abc123/backup/chats");
    std::fs::create_dir_all(&nested_dir).unwrap();
    let file_path = nested_dir.join("nested.json");
    std::fs::write(&file_path, json).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 0);
}

#[test]
fn test_parse_gemini_tokens_with_camel_case_aliases() {
    let json = r#"{
        "sessionId": "ses_alias",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-3-flash-preview",
                "tokens": {
                    "promptTokenCount": 100,
                    "candidatesTokenCount": 50,
                    "cachedContentTokenCount": 20,
                    "totalTokenCount": 150
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
    assert_eq!(messages[0].model_id, "gemini-3-flash-preview");
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.total(), 150);
}

#[test]
fn test_parse_gemini_tokens_with_snake_case_aliases() {
    let json = r#"{
        "sessionId": "ses_snake",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-3-flash-preview",
                "tokens": {
                    "prompt": 200,
                    "candidates": 80,
                    "cached_tokens": 30,
                    "reasoning": 10,
                    "tool_tokens": 5,
                    "total_tokens": 295
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
    assert_eq!(messages[0].tokens.input, 175);
    assert_eq!(messages[0].tokens.output, 80);
    assert_eq!(messages[0].tokens.cache_read, 30);
    assert_eq!(messages[0].tokens.reasoning, 10);
    assert_eq!(messages[0].tokens.total(), 295);
}

#[test]
fn test_parse_gemini_session_non_gemini_type_with_tokens() {
    let json = r#"{
        "sessionId": "ses_nongemini",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "assistant",
                "model": "gemini-3-flash-preview",
                "tokens": {
                    "input": 150,
                    "output": 40,
                    "cached": 10,
                    "total": 190
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
    assert_eq!(messages[0].model_id, "gemini-3-flash-preview");
    assert_eq!(messages[0].tokens.input, 140);
    assert_eq!(messages[0].tokens.output, 40);
    assert_eq!(messages[0].tokens.cache_read, 10);
    assert_eq!(messages[0].tokens.total(), 190);
}

#[test]
fn test_parse_gemini_valid_path_without_gemini_component() {
    let json = r#"{
        "sessionId": "ses_custom",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-2.0-flash",
                "tokens": {
                    "input": 10,
                    "output": 20
                }
            }
        ]
    }"#;

    let dir = TempDir::new().unwrap();
    let base = dir.path();
    let chats_dir = base.join("custom_home/tmp/abc123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("session.json");
    std::fs::write(&file_path, json).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.0-flash");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 20);
}

#[test]
fn test_parse_gemini_stream_jsonl_direct_tokens_without_gemini_prefix() {
    let content = r#"{"sessionId":"ses-nogem","projectHash":"abc123","startTime":"2026-05-01T00:00:00.000Z","lastUpdated":"2026-05-01T00:01:00.000Z"}
{"id":"msg-1","timestamp":"2026-05-01T00:01:00.000Z","type":"gemini","model":"gemini-3.1-pro-preview","tokens":{"input":500,"output":30,"cached":0,"thoughts":100,"tool":5,"total":635}}"#;
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join("my_gemini/tmp/456/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("session.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "ses-nogem");
    assert_eq!(messages[0].model_id, "gemini-3.1-pro-preview");
    assert_eq!(messages[0].tokens.input, 505);
    assert_eq!(messages[0].tokens.output, 30);
    assert_eq!(messages[0].tokens.cache_read, 0);
    assert_eq!(messages[0].tokens.reasoning, 100);
    assert_eq!(messages[0].tokens.total(), 635);
}

#[test]
fn test_parse_headless_jsonl_non_gemini_type_with_direct_tokens() {
    let content = r#"{"type":"init","model":"gemini-3-flash-preview","session_id":"session-tokens"}
{"type":"result","id":"msg-1","tokens":{"input":100,"output":25,"cached":10,"total":125}}"#;
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join("custom_root/tmp/789/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("session.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-3-flash-preview");
    assert_eq!(messages[0].tokens.input, 90);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 10);
    assert_eq!(messages[0].tokens.total(), 125);
}

#[test]
fn test_parse_gemini_tokens_with_mixed_duplicate_fields() {
    let json = r#"{
        "sessionId": "ses_dup",
        "projectHash": "abc123",
        "startTime": "2025-06-15T12:00:00Z",
        "lastUpdated": "2025-06-15T12:30:00Z",
        "messages": [
            {
                "id": "msg_1",
                "timestamp": "2025-06-15T12:01:00Z",
                "type": "gemini",
                "model": "gemini-3-flash-preview",
                "tokens": {
                    "input": 100,
                    "prompt": 200,
                    "output": 50,
                    "candidates": 60,
                    "cached": 5,
                    "total": 215
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
    assert_eq!(messages[0].model_id, "gemini-3-flash-preview");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 5);
}
