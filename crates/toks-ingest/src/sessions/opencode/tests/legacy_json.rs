use super::*;

#[test]
fn test_parse_opencode_structure() {
    let json = r#"{
        "id": "msg_123",
        "sessionID": "ses_456",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 100,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let mut bytes = json.as_bytes().to_vec();
    let msg: OpenCodeMessage = simd_json::from_slice(&mut bytes).unwrap();

    assert_eq!(msg.model_id, Some("claude-sonnet-4".to_string()));
    assert_eq!(msg.tokens.unwrap().input, 1000);
    assert_eq!(msg.agent, None);
}

#[test]
fn test_parse_opencode_with_agent() {
    let json = r#"{
        "id": "msg_123",
        "sessionID": "ses_456",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "agent": "OmO",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 100,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let mut bytes = json.as_bytes().to_vec();
    let msg: OpenCodeMessage = simd_json::from_slice(&mut bytes).unwrap();

    assert_eq!(msg.agent, Some("OmO".to_string()));
}

/// Verify negative token values are clamped to 0 (defense-in-depth for PR #147)
#[test]
fn test_negative_values_clamped_to_zero() {
    use std::io::Write;

    let json = r#"{
        "id": "msg_negative",
        "sessionID": "ses_negative",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": -0.05,
        "tokens": {
            "input": -100,
            "output": -50,
            "reasoning": -25,
            "cache": { "read": -200, "write": -10 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let mut temp_file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    temp_file.write_all(json.as_bytes()).unwrap();

    let result = parse_opencode_file(temp_file.path());
    assert!(result.is_some(), "Should parse file with negative values");

    let msg = result.unwrap();
    assert_eq!(msg.tokens.input, 0, "Negative input should be clamped to 0");
    assert_eq!(
        msg.tokens.output, 0,
        "Negative output should be clamped to 0"
    );
    assert_eq!(
        msg.tokens.cache_read, 0,
        "Negative cache_read should be clamped to 0"
    );
    assert_eq!(
        msg.tokens.cache_write, 0,
        "Negative cache_write should be clamped to 0"
    );
    assert_eq!(
        msg.tokens.reasoning, 0,
        "Negative reasoning should be clamped to 0"
    );
    assert!(
        msg.cost >= 0.0,
        "Negative cost should be clamped to 0.0, got {}",
        msg.cost
    );
}

#[test]
fn test_parse_opencode_file_requires_explicit_assistant_role() {
    use std::io::Write;
    // Regression: making `role` optional for the v2 SQLite path must NOT
    // loosen file parsing. A file without a `role` (or a non-assistant one)
    // is not assistant usage and must be skipped -- the missing-role =>
    // assistant shortcut applies only to the type-filtered session_message
    // SQLite query, never to JSON files.
    let role_less = r#"{
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 10, "output": 5, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;
    let mut f1 = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    f1.write_all(role_less.as_bytes()).unwrap();
    assert!(
        parse_opencode_file(f1.path()).is_none(),
        "a role-less OpenCode JSON file must not be counted as assistant usage"
    );

    let user_role = r#"{
        "role": "user",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 10, "output": 5, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;
    let mut f2 = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    f2.write_all(user_role.as_bytes()).unwrap();
    assert!(
        parse_opencode_file(f2.path()).is_none(),
        "a non-assistant OpenCode JSON file must be skipped"
    );
}

/// JSON dedup_key uses msg.id when present
#[test]
fn test_dedup_key_from_json_message_id() {
    use std::io::Write;

    let json = r#"{
        "id": "msg_dedup_001",
        "sessionID": "ses_001",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.01,
        "tokens": {
            "input": 100,
            "output": 50,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let mut temp_file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    temp_file.write_all(json.as_bytes()).unwrap();

    let msg = parse_opencode_file(temp_file.path()).expect("Should parse");
    assert_eq!(
        msg.dedup_key,
        Some("msg_dedup_001".to_string()),
        "dedup_key should use msg.id from JSON"
    );
}

#[test]
fn test_parse_opencode_file_sets_duration_from_completed_time() {
    use std::io::Write;

    let json = r#"{
        "id": "msg_timed",
        "sessionID": "ses_001",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.01,
        "tokens": {
            "input": 100,
            "output": 50,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0, "completed": 1700000001234.0 }
    }"#;

    let mut temp_file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    temp_file.write_all(json.as_bytes()).unwrap();

    let msg = parse_opencode_file(temp_file.path()).expect("Should parse");
    assert_eq!(msg.duration_ms, Some(1234));
}

/// JSON dedup_key falls back to file stem when msg.id is absent
#[test]
fn test_dedup_key_falls_back_to_file_stem() {
    let json = r#"{
        "sessionID": "ses_001",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.01,
        "tokens": {
            "input": 100,
            "output": 50,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("msg_fallback_999.json");
    std::fs::write(&file_path, json).unwrap();

    let msg = parse_opencode_file(&file_path).expect("Should parse");
    assert_eq!(
        msg.dedup_key,
        Some("msg_fallback_999".to_string()),
        "dedup_key should fall back to file stem when id is missing"
    );
}

/// Non-assistant messages are skipped (no dedup_key produced)
#[test]
fn test_dedup_key_skips_non_assistant() {
    let json = r#"{
        "id": "msg_user_001",
        "sessionID": "ses_001",
        "role": "user",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": {
            "input": 100,
            "output": 50,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("msg_user_001.json");
    std::fs::write(&file_path, json).unwrap();

    let result = parse_opencode_file(&file_path);
    assert!(result.is_none(), "User messages should be skipped");
}

/// SQLite dedup_key falls back to the database row id when the message has no embedded id.
#[test]
fn test_sqlite_dedup_key_falls_back_to_row_id() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");

    let conn = create_opencode_sqlite_db(&db_path);

    let data_json = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 0,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_sqlite_001", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedup_key,
        Some("msg_sqlite_001".to_string()),
        "SQLite dedup_key should fall back to the row id when no embedded id exists"
    );
    assert_eq!(messages[0].model_id, "claude-sonnet-4");
    assert_eq!(messages[0].tokens.input, 1000);
}

#[test]
fn test_parse_opencode_file_marks_positive_cost_as_provider_reported() {
    use std::io::Write;
    let json = r#"{
        "id": "msg_cost_001",
        "sessionID": "ses_cost",
        "role": "assistant",
        "modelID": "z-ai/glm-4.6",
        "providerID": "openrouter",
        "cost": 0.0025158,
        "tokens": {
            "input": 2675,
            "output": 28,
            "reasoning": 1,
            "cache": { "read": 7700, "write": 0 }
        },
        "time": { "created": 1765915142201.0 }
    }"#;

    let mut temp_file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    temp_file.write_all(json.as_bytes()).unwrap();

    let msg = parse_opencode_file(temp_file.path()).expect("Should parse");
    assert_eq!(
        msg.cost_source,
        crate::sessions::CostSource::ProviderReported,
        "positive embedded cost must survive the LiteLLM repricing pass"
    );
    assert!((msg.cost - 0.0025158).abs() < 1e-12);
}

#[test]
fn test_parse_opencode_file_keeps_zero_cost_unknown_for_estimation() {
    use std::io::Write;
    let json = r#"{
        "id": "msg_cost_002",
        "sessionID": "ses_cost",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.0,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    let mut temp_file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    temp_file.write_all(json.as_bytes()).unwrap();

    let msg = parse_opencode_file(temp_file.path()).expect("Should parse");
    assert_eq!(
        msg.cost_source,
        crate::sessions::CostSource::Unknown,
        "zero cost means OpenCode had no pricing — leave repricing enabled"
    );
}
