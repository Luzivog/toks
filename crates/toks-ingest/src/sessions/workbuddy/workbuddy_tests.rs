use super::*;
use rusqlite::{params, Connection};

fn create_workbuddy_db(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            cwd TEXT,
            model TEXT
        );
        CREATE TABLE session_usage (
            session_id TEXT PRIMARY KEY,
            used INTEGER,
            size INTEGER,
            updated_at INTEGER,
            credit_json TEXT
        );
        "#,
    )
    .unwrap();
    conn
}

#[test]
fn parse_workbuddy_sqlite_reads_session_usage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("workbuddy.db");
    let conn = create_workbuddy_db(&db_path);
    conn.execute(
        "INSERT INTO sessions (id, cwd, model) VALUES (?1, ?2, ?3)",
        params!["session-1", "/Users/alice/project", "deepseek-v4-pro"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_usage (session_id, used, size, updated_at, credit_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["session-1", 1234, 1000000, 1_780_000_000_000_i64, "{}"],
    )
    .unwrap();
    drop(conn);

    let messages = parse_workbuddy_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "workbuddy");
    assert_eq!(message.model_id, "deepseek-v4-pro");
    assert_eq!(message.provider_id, "deepseek");
    assert_eq!(message.session_id, "session-1");
    assert_eq!(message.tokens.input, 1234);
    assert_eq!(message.tokens.output, 0);
    assert_eq!(message.message_count, 1);
    assert_eq!(message.workspace_label.as_deref(), Some("project"));
    assert_eq!(
        message.dedup_key.as_deref(),
        Some("workbuddy:session-1:1780000000000")
    );
}

#[test]
fn parse_workbuddy_sqlite_skips_zero_usage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("workbuddy.db");
    let conn = create_workbuddy_db(&db_path);
    conn.execute(
        "INSERT INTO session_usage (session_id, used, size, updated_at, credit_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["empty-session", 0, 1000000, 1_780_000_000_000_i64, "{}"],
    )
    .unwrap();
    drop(conn);

    assert!(parse_workbuddy_sqlite(&db_path).is_empty());
}

#[test]
fn parse_workbuddy_file_reads_jsonl_function_call_usage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-1.jsonl");
    std::fs::write(
        &path,
        // The reported total proves prompt_tokens includes the cached hit,
        // so the parser can safely split the cache-exclusive input.
        r#"{"id":"call-1","timestamp":1780000000100,"type":"function_call","sessionId":"session-1","cwd":"/Users/alice/admin-panel","providerData":{"requestModelId":"glm-5.2","messageId":"msg-1","rawUsage":{"prompt_tokens":140732,"completion_tokens":635,"total_tokens":141367,"prompt_cache_hit_tokens":76032}}}"#,
    )
    .unwrap();

    let messages = parse_workbuddy_file(&path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "workbuddy");
    assert_eq!(message.model_id, "glm-5.2");
    assert_eq!(message.tokens.input, 64700);
    assert_eq!(message.tokens.output, 635);
    assert_eq!(message.tokens.cache_read, 76032);
    assert_eq!(message.tokens.total(), 141367);
    assert_eq!(message.workspace_label.as_deref(), Some("admin-panel"));
}
