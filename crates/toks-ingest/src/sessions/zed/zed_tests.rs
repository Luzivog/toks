use super::*;
use rusqlite::{params, Connection};
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn create_threads_db(dir: &TempDir) -> (std::path::PathBuf, Connection) {
    let db_path = dir.path().join("threads.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            summary TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            data_type TEXT NOT NULL,
            data BLOB NOT NULL,
            parent_id TEXT,
            folder_paths TEXT,
            folder_paths_order TEXT,
            created_at TEXT
        );
        "#,
    )
    .unwrap();
    (db_path, conn)
}

fn thread_json(provider: &str, model: &str, request_token_usage: Value) -> String {
    json!({
        "version": "0.3.0",
        "title": "Test thread",
        "messages": [],
        "updated_at": "2026-05-01T12:30:00Z",
        "request_token_usage": request_token_usage,
        "cumulative_token_usage": {
            "input_tokens": 999,
            "output_tokens": 999
        },
        "model": {
            "provider": provider,
            "model": model
        },
        "imported": false
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn insert_thread(
    conn: &Connection,
    id: &str,
    json: &str,
    data_type: &str,
    updated_at: &str,
    created_at: Option<&str>,
    folder_paths: Option<&str>,
    folder_paths_order: Option<&str>,
) {
    let data = match data_type {
        "zstd" => zstd::encode_all(json.as_bytes(), 3).unwrap(),
        "json" => json.as_bytes().to_vec(),
        _ => panic!("unsupported test data_type"),
    };

    conn.execute(
        r#"
        INSERT INTO threads (
            id, summary, updated_at, data_type, data, created_at, folder_paths, folder_paths_order
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            id,
            "Test thread",
            updated_at,
            data_type,
            data,
            created_at,
            folder_paths,
            folder_paths_order
        ],
    )
    .unwrap();
}

#[test]
fn parse_zed_sqlite_reads_zstd_hosted_thread_usage() {
    let dir = TempDir::new().unwrap();
    let (db_path, conn) = create_threads_db(&dir);
    let payload = thread_json(
        ZED_HOSTED_PROVIDER,
        "claude-sonnet-4-5",
        json!({
            "user-1": {
                "input_tokens": 100,
                "output_tokens": 20,
                "cache_creation_input_tokens": 5,
                "cache_read_input_tokens": 10
            },
            "user-2": {
                "input_tokens": 50,
                "output_tokens": 7
            }
        }),
    );
    insert_thread(
        &conn,
        "thread-1",
        &payload,
        "zstd",
        "2026-05-01T12:30:00Z",
        Some("2026-05-01T12:00:00Z"),
        Some("/workspace/a\n/workspace/b"),
        Some("1,0"),
    );

    let messages = parse_zed_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "zed");
    assert_eq!(message.provider_id, ZED_HOSTED_PROVIDER);
    assert_eq!(message.model_id, "claude-sonnet-4-5");
    assert_eq!(message.session_id, "thread-1");
    assert_eq!(
        message.timestamp,
        parse_timestamp_str("2026-05-01T12:00:00Z").unwrap()
    );
    assert_eq!(message.tokens.input, 150);
    assert_eq!(message.tokens.output, 27);
    assert_eq!(message.tokens.cache_write, 5);
    assert_eq!(message.tokens.cache_read, 10);
    assert_eq!(message.message_count, 2);
    assert_eq!(message.workspace_key.as_deref(), Some("/workspace/b"));
    assert_eq!(message.workspace_label.as_deref(), Some("b"));
    assert_eq!(message.dedup_key.as_deref(), Some("zed:thread-1"));
}

#[test]
fn parse_zed_sqlite_skips_non_hosted_threads() {
    let dir = TempDir::new().unwrap();
    let (db_path, conn) = create_threads_db(&dir);
    let payload = thread_json(
        "anthropic",
        "claude-sonnet-4-5",
        json!({
            "user-1": {
                "input_tokens": 100,
                "output_tokens": 20
            }
        }),
    );
    insert_thread(
        &conn,
        "thread-1",
        &payload,
        "zstd",
        "2026-05-01T12:30:00Z",
        None,
        None,
        None,
    );

    assert!(parse_zed_sqlite(&db_path).is_empty());
}

#[test]
fn parse_zed_sqlite_uses_cumulative_usage_when_request_usage_is_absent() {
    let dir = TempDir::new().unwrap();
    let (db_path, conn) = create_threads_db(&dir);
    let payload = json!({
        "version": "0.3.0",
        "title": "Test thread",
        "messages": [],
        "updated_at": "2026-05-01T12:30:00Z",
        "request_token_usage": {},
        "cumulative_token_usage": {
            "input_tokens": 12,
            "output_tokens": 3,
            "cache_creation_input_tokens": 2,
            "cache_read_input_tokens": 4
        },
        "model": {
            "provider": ZED_HOSTED_PROVIDER,
            "model": "gpt-5.2"
        },
        "imported": false
    })
    .to_string();
    insert_thread(
        &conn,
        "thread-1",
        &payload,
        "json",
        "2026-05-01T12:30:00Z",
        None,
        None,
        None,
    );

    let messages = parse_zed_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 12);
    assert_eq!(messages[0].tokens.output, 3);
    assert_eq!(messages[0].tokens.cache_write, 2);
    assert_eq!(messages[0].tokens.cache_read, 4);
    assert_eq!(messages[0].message_count, 1);
}

#[test]
fn parse_zed_sqlite_supports_pre_created_at_schema() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("threads.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            summary TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            data_type TEXT NOT NULL,
            data BLOB NOT NULL
        );
        "#,
    )
    .unwrap();
    let payload = thread_json(
        ZED_HOSTED_PROVIDER,
        "gpt-5.2",
        json!({
            "user-1": {
                "input_tokens": 12,
                "output_tokens": 3
            }
        }),
    );
    let data = zstd::encode_all(payload.as_bytes(), 3).unwrap();
    conn.execute(
        "INSERT INTO threads (id, summary, updated_at, data_type, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["thread-1", "Test thread", "2026-05-01T12:30:00Z", "zstd", data],
    )
    .unwrap();

    let messages = parse_zed_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].timestamp,
        parse_timestamp_str("2026-05-01T12:30:00Z").unwrap()
    );
}

#[test]
fn workspace_key_from_folders_uses_original_order_when_available() {
    assert_eq!(
        workspace_key_from_folders(Some("/sorted/a\n/sorted/b"), Some("1,0")).as_deref(),
        Some("/sorted/b")
    );
    assert_eq!(
        workspace_key_from_folders(Some("/sorted/a\n/sorted/b"), None).as_deref(),
        Some("/sorted/a")
    );
}

#[test]
fn decode_thread_json_rejects_unknown_data_type() {
    let err = decode_thread_json("brotli", b"{}").unwrap_err();
    assert!(err.contains("unsupported data_type"));
}

#[test]
fn parse_zed_sqlite_returns_empty_for_missing_database() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.db");
    assert!(parse_zed_sqlite(&missing).is_empty());
    fs::create_dir_all(dir.path().join("threads")).unwrap();
}
