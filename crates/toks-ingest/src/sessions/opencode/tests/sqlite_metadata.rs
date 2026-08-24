use super::*;

#[test]
fn test_parse_opencode_sqlite_marks_positive_cost_as_provider_reported() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("test_opencode_cost.db");
    let conn = create_opencode_sqlite_db(&db_path);

    let costed = r#"{
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
    let free = r#"{
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

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_costed", "ses_cost", costed],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_free", "ses_cost", free],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 2);

    let costed_msg = messages
        .iter()
        .find(|m| m.dedup_key.as_deref() == Some("msg_costed"))
        .unwrap();
    assert_eq!(
        costed_msg.cost_source,
        crate::sessions::CostSource::ProviderReported
    );

    let free_msg = messages
        .iter()
        .find(|m| m.dedup_key.as_deref() == Some("msg_free"))
        .unwrap();
    assert_eq!(free_msg.cost_source, crate::sessions::CostSource::Unknown);
}

#[test]
fn test_parse_opencode_file_uses_explicit_path_root_as_workspace() {
    let json = r#"{
        "id": "msg_workspace_001",
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
        "time": { "created": 1700000000000.0 },
        "path": { "root": "/Users/alice/opencode-json-repo" }
    }"#;

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("msg_workspace_001.json");
    std::fs::write(&file_path, json).unwrap();

    let msg = parse_opencode_file(&file_path).expect("Should parse");
    assert_eq!(
        msg.workspace_key.as_deref(),
        Some("/Users/alice/opencode-json-repo")
    );
    assert_eq!(msg.workspace_label.as_deref(), Some("opencode-json-repo"));
}

#[test]
fn test_parse_opencode_file_ignores_non_object_path_without_rejecting_message() {
    let json = r#"{
        "id": "msg_path_string_001",
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
        "time": { "created": 1700000000000.0 },
        "path": "/Users/alice/not-object"
    }"#;

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("msg_path_string_001.json");
    std::fs::write(&file_path, json).unwrap();

    let msg = parse_opencode_file(&file_path).expect("Should parse");
    assert_eq!(msg.workspace_key, None);
    assert_eq!(msg.workspace_label, None);
}

#[test]
fn test_parse_opencode_sqlite_uses_session_directory_as_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");

    let conn = create_opencode_sqlite_db(&db_path);
    conn.execute_batch(
        "CREATE TABLE session (
            id TEXT PRIMARY KEY,
            directory TEXT NOT NULL,
            title TEXT
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, directory) VALUES (?1, ?2)",
        rusqlite::params!["ses_001", "/Users/alice/opencode-sqlite-repo"],
    )
    .unwrap();

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
        rusqlite::params!["msg_sqlite_workspace", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/alice/opencode-sqlite-repo")
    );
    assert_eq!(
        messages[0].workspace_label.as_deref(),
        Some("opencode-sqlite-repo")
    );
}

#[test]
fn test_parse_opencode_sqlite_legacy_fallback_uses_path_root_when_session_table_missing() {
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
        "time": { "created": 1700000000000.0 },
        "path": { "root": "/Users/alice/legacy-fallback-repo" }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_sqlite_legacy_workspace", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/alice/legacy-fallback-repo")
    );
    assert_eq!(
        messages[0].workspace_label.as_deref(),
        Some("legacy-fallback-repo")
    );
    assert_eq!(messages[0].tokens.input, 1000);
}

#[test]
fn test_parse_opencode_sqlite_duplicate_workspace_conflict_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");

    let conn = create_opencode_sqlite_db(&db_path);
    conn.execute_batch(
        "CREATE TABLE session (
            id TEXT PRIMARY KEY,
            directory TEXT NOT NULL,
            title TEXT
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, directory) VALUES (?1, ?2)",
        rusqlite::params!["ses_root", "/Users/alice/root-workspace"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, directory) VALUES (?1, ?2)",
        rusqlite::params!["ses_fork", "/Users/alice/fork-workspace"],
    )
    .unwrap();

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
        "time": { "created": 1700000000000.0, "completed": 1700000000500.0 },
        "mode": "build"
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["z_root_copy", "ses_root", data_json],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["a_fork_copy", "ses_fork", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key, None);
    assert_eq!(messages[0].workspace_label, None);
    assert_eq!(messages[0].tokens.input, 1000);
}

/// SQLite prefers the embedded message id when present so JSON/SQLite overlap keeps deduplicating.
#[test]
fn test_sqlite_dedup_key_prefers_embedded_message_id() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_opencode.db");

    let conn = create_opencode_sqlite_db(&db_path);

    let valid = r#"{
        "id": "embedded_msg_001",
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": { "input": 100, "output": 50, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["row_msg_001", "ses_001", valid],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedup_key,
        Some("embedded_msg_001".to_string()),
        "SQLite dedup_key should prefer the embedded message id for cross-source overlap"
    );
}
