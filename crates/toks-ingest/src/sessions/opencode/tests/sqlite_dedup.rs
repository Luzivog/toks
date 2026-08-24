use super::*;

/// SQLite skips rows without tokens or with non-assistant role
#[test]
fn test_sqlite_skips_invalid_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");

    let conn = create_opencode_sqlite_db(&db_path);

    let valid = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 100, "output": 50, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;

    let user_msg = r#"{
        "role": "user",
        "modelID": "claude-sonnet-4",
        "time": { "created": 1700000000000.0 }
    }"#;

    let no_tokens = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_valid", "ses_001", valid],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_user", "ses_001", user_msg],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_no_tokens", "ses_001", no_tokens],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(
        messages.len(),
        1,
        "Should only parse valid assistant message"
    );
    assert_eq!(messages[0].dedup_key, Some("msg_valid".to_string()));
}

/// Forked SQLite sessions should not count copied history more than once.
#[test]
fn test_sqlite_deduplicates_forked_history_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");
    let conn = create_opencode_sqlite_db(&db_path);

    let root_msg = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 25,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0, "completed": 1700000000500.0 },
        "mode": "build"
    }"#;

    let new_msg = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.08,
        "tokens": {
            "input": 1300,
            "output": 650,
            "reasoning": 40,
            "cache": { "read": 100, "write": 0 }
        },
        "time": { "created": 1700000001000.0, "completed": 1700000001500.0 },
        "mode": "build"
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["root_row", "root_session", root_msg],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["fork_copy_row", "fork_session", root_msg],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["fork_new_row", "fork_session", new_msg],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(
        messages.len(),
        2,
        "Forked copies of the same assistant history should collapse inside SQLite parsing"
    );
    assert_eq!(messages[0].tokens.input, 1000);
    assert_eq!(messages[1].tokens.input, 1300);
}

/// Same-timestamp messages with different payloads should remain distinct.
#[test]
fn test_sqlite_same_timestamp_distinct_payloads_survive() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");
    let conn = create_opencode_sqlite_db(&db_path);

    let first = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0, "completed": 1700000000100.0 },
        "mode": "build"
    }"#;

    let second = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "cost": 0.05,
        "tokens": {
            "input": 1500,
            "output": 750,
            "reasoning": 0,
            "cache": { "read": 0, "write": 0 }
        },
        "time": { "created": 1700000000000.0, "completed": 1700000000100.0 },
        "mode": "build"
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["row_one", "session_one", first],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["row_two", "session_two", second],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(
        messages.len(),
        2,
        "Distinct assistant calls should survive even when they share the same creation timestamp"
    );
}

/// Cross-source dedup: matching IDs between SQLite and JSON should deduplicate
#[test]
fn test_cross_source_dedup_by_message_id() {
    use std::collections::HashSet;

    let dir = tempfile::tempdir().unwrap();

    // --- SQLite source ---
    let db_path = dir.path().join("opencode.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            data TEXT NOT NULL
        );",
    )
    .unwrap();

    let shared_data_json = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 500, "output": 200, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;
    let sqlite_only_data_json = r#"{
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 700, "output": 250, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000001000.0 }
    }"#;

    // Insert two messages into SQLite
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_shared_001", "ses_001", shared_data_json],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_sqlite_only", "ses_001", sqlite_only_data_json],
    )
    .unwrap();
    drop(conn);

    // --- JSON source ---
    let json_dir = dir.path().join("json");
    std::fs::create_dir_all(&json_dir).unwrap();

    // Duplicate of SQLite msg_shared_001
    let json_shared = r#"{
        "id": "msg_shared_001",
        "sessionID": "ses_001",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 500, "output": 200, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;
    std::fs::write(json_dir.join("msg_shared_001.json"), json_shared).unwrap();

    // JSON-only message (not in SQLite)
    let json_only = r#"{
        "id": "msg_json_only",
        "sessionID": "ses_001",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 100, "output": 50, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;
    std::fs::write(json_dir.join("msg_json_only.json"), json_only).unwrap();

    // --- Simulate the dedup logic from lib.rs ---
    let sqlite_messages = parse_opencode_sqlite(&db_path);
    assert_eq!(sqlite_messages.len(), 2);

    // Build seen set from SQLite (same as lib.rs)
    let mut seen: HashSet<String> = HashSet::new();
    for msg in &sqlite_messages {
        if let Some(ref key) = msg.dedup_key {
            seen.insert(key.clone());
        }
    }
    assert_eq!(seen.len(), 2);

    // Parse JSON files
    let json_msg_shared = parse_opencode_file(&json_dir.join("msg_shared_001.json")).unwrap();
    let json_msg_only = parse_opencode_file(&json_dir.join("msg_json_only.json")).unwrap();

    // Filter JSON through seen set (same logic as lib.rs)
    let json_messages = vec![json_msg_shared, json_msg_only];
    let deduped: Vec<UnifiedMessage> = json_messages
        .into_iter()
        .filter(|msg| {
            msg.dedup_key
                .as_ref()
                .is_none_or(|key| seen.insert(key.clone()))
        })
        .collect();

    // msg_shared_001 should be filtered (duplicate), msg_json_only should survive
    assert_eq!(
        deduped.len(),
        1,
        "Only the JSON-only message should survive dedup"
    );
    assert_eq!(
        deduped[0].dedup_key,
        Some("msg_json_only".to_string()),
        "Surviving message should be the JSON-only one"
    );

    // Total unique messages = 2 from SQLite + 1 from JSON
    let total = sqlite_messages.len() + deduped.len();
    assert_eq!(total, 3, "Should have 3 unique messages total");
}
