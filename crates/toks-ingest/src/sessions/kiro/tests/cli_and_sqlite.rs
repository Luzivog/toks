use super::*;

#[test]
fn test_parse_kiro_estimates_tokens_from_jsonl_content() {
    let dir = TempDir::new().unwrap();
    let json = r#"{"session_id":"session-1","cwd":"/tmp/project","session_state":{"rts_model_state":{"model_info":{"model_id":"claude-sonnet-4-5"}},"conversation_metadata":{"user_turn_metadatas":[{"input_token_count":0,"output_token_count":0,"turn_duration":123,"end_timestamp":1770983427,"total_request_count":2,"message_ids":["prompt-1","assistant-1"]}]}}}"#;
    let jsonl = r#"{"version":"v1","kind":"Prompt","data":{"message_id":"prompt-1","content":[{"kind":"text","data":"hello world"}],"meta":{"timestamp":1770983426.420942}}}
{"version":"v1","kind":"AssistantMessage","data":{"message_id":"assistant-1","content":[{"kind":"text","data":"response text"}]}}"#;
    let path = create_session_files(&dir, "session-1", json, jsonl);

    let messages = parse_kiro_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "kiro");
    assert_eq!(messages[0].provider_id, "amazon-bedrock");
    assert_eq!(messages[0].model_id, "claude-sonnet-4-5");
    assert_eq!(messages[0].session_id, "session-1");
    assert_eq!(messages[0].tokens.input, 3);
    assert_eq!(messages[0].tokens.output, 4);
    assert_eq!(messages[0].message_count, 2);
    assert!(messages[0].is_turn_start);
    assert_eq!(messages[0].timestamp, 1770983426420);
    assert_eq!(messages[0].duration_ms, Some(580));
    assert_eq!(messages[0].workspace_key, Some("/tmp/project".to_string()));
    assert_eq!(messages[0].workspace_label, Some("project".to_string()));
}

#[test]
fn test_parse_kiro_skips_zero_content_turns() {
    let dir = TempDir::new().unwrap();
    let json = r#"{"session_id":"session-2","cwd":"/tmp","session_state":{"rts_model_state":{"model_info":{"model_id":"model"}},"conversation_metadata":{"user_turn_metadatas":[{"input_token_count":0,"output_token_count":0,"message_ids":["missing"]}]}}}"#;
    let jsonl = "";
    let path = create_session_files(&dir, "session-2", json, jsonl);

    let messages = parse_kiro_file(&path);

    assert!(messages.is_empty());
}

#[test]
fn test_parse_kiro_skips_malformed_jsonl_lines() {
    let dir = TempDir::new().unwrap();
    let json = r#"{"session_id":"session-3","cwd":"/tmp/project","session_state":{"rts_model_state":{"model_info":{"model_id":"claude-sonnet-4-5"}},"conversation_metadata":{"user_turn_metadatas":[{"input_token_count":0,"output_token_count":0,"turn_duration":100,"end_timestamp":1770983427,"total_request_count":2,"message_ids":["prompt-3","assistant-3"]}]}}}"#;
    let jsonl = r#"{"version":"v1","kind":"Prompt","data":{"message_id":"prompt-3","content":[{"kind":"text","data":"hello world"}],"meta":{"timestamp":1770983426.420942}}}
not valid json at all
{"version":"v1","kind":"AssistantMessage","data":{"message_id":"assistant-3","content":[{"kind":"text","data":"response text"}]}}"#;
    let path = create_session_files(&dir, "session-3", json, jsonl);

    let messages = parse_kiro_file(&path);

    assert_eq!(messages.len(), 1);
    assert!(messages[0].tokens.input > 0 || messages[0].tokens.output > 0);
}

#[test]
fn test_parse_kiro_sqlite_sets_duration_from_request_metadata() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("data.sqlite3");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE conversations_v2 (key TEXT, conversation_id TEXT, value TEXT)",
        [],
    )
    .unwrap();
    let value = r#"{
            "model_info": {
                "model_id": "auto",
                "context_window_tokens": 1000
            },
            "history": [{
                "request_metadata": {
                    "context_usage_percentage": 10,
                    "response_size": 40,
                    "request_start_timestamp_ms": 1770983426000,
                    "stream_end_timestamp_ms": 1770983427500
                }
            }]
        }"#;
    conn.execute(
        "INSERT INTO conversations_v2 (key, conversation_id, value) VALUES (?1, ?2, ?3)",
        (&"/tmp/project", &"conv-1", &value),
    )
    .unwrap();
    drop(conn);

    let messages = parse_kiro_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "auto");
    assert_eq!(messages[0].timestamp, 1770983426000);
    assert_eq!(messages[0].duration_ms, Some(1500));
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 10);
}
