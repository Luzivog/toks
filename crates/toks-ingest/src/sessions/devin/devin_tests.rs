use super::*;
use rusqlite::Connection;
use std::io::Write;
use tempfile::TempDir;

fn create_devin_cli_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("sessions.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                working_directory TEXT NOT NULL,
                backend_type TEXT NOT NULL,
                model TEXT NOT NULL,
                title TEXT,
                agent_mode TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_activity_at INTEGER NOT NULL
            );
            CREATE TABLE message_nodes (
                row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                node_id INTEGER NOT NULL,
                parent_node_id INTEGER,
                chat_message TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                metadata TEXT
            );
            "#,
    )
    .unwrap();
    db_path
}

fn insert_session(conn: &Connection, id: &str, working_directory: &str, model: &str) {
    conn.execute(
            "INSERT INTO sessions (id, working_directory, backend_type, model, title, agent_mode, created_at, last_activity_at) VALUES (?1, ?2, 'windsurf', ?3, NULL, 'accept-edits', 1, 1)",
            rusqlite::params![id, working_directory, model],
        )
        .unwrap();
}

fn set_session_title(conn: &Connection, id: &str, title: &str) {
    conn.execute(
        "UPDATE sessions SET title = ?2 WHERE id = ?1",
        rusqlite::params![id, title],
    )
    .unwrap();
}

/// Insert a message_nodes row. In real Devin CLI databases the SQL
/// `metadata` column is always NULL; token metrics and generation_model
/// live inside the `chat_message` JSON blob under `$.metadata`.
fn insert_message(conn: &Connection, session_id: &str, chat_message: &str, created_at: i64) -> i64 {
    conn.execute(
        "INSERT INTO message_nodes (session_id, node_id, chat_message, metadata, created_at) VALUES (?1, 1, ?2, NULL, ?3)",
        rusqlite::params![session_id, chat_message, created_at],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn test_parse_devin_cli_sqlite_reads_assistant_metrics() {
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // sessions.model is "adaptive" (a routing mode), but the real model
    // is in chat_message.metadata.generation_model.
    insert_session(&conn, "sess-1", "/Users/alice/project", "adaptive");
    let chat = r#"{"role":"assistant","content":"hello","metadata":{"num_tokens":147,"generation_model":"glm-5-2-max-1m","metrics":{"input_tokens":31134,"output_tokens":147,"cache_read_tokens":8,"cache_creation_tokens":null,"total_time_ms":2846}}}"#;
    insert_message(&conn, "sess-1", chat, 1_700_000_000);
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.client, "devin-cli");
    assert_eq!(msg.session_id, "sess-1");
    assert_eq!(msg.model_id, "glm-5-2-max-1m");
    // `inferred_provider_from_model` recognizes "glm" and infers "zai"
    // (Zhipu AI), taking precedence over the "devin" fallback below —
    // the same convention this file already applies to Claude/GPT
    // models (see the "anthropic" assertion further down). "devin" is
    // only used when inference can't identify the model at all.
    assert_eq!(msg.provider_id, "zai");
    assert_eq!(msg.tokens.input, 31134);
    assert_eq!(msg.tokens.output, 147);
    assert_eq!(msg.tokens.cache_read, 8);
    assert_eq!(msg.tokens.cache_write, 0);
    // `created_at` is the message row's write time (the turn's end), so
    // the message timestamp is back-calculated to the turn start:
    // created_at_ms - total_time_ms. See #890 (follow-up).
    assert_eq!(msg.timestamp, 1_700_000_000_000 - 2846);
    assert_eq!(msg.duration_ms, Some(2846));
    assert_eq!(msg.workspace_key.as_deref(), Some("/Users/alice/project"));
}

#[test]
fn test_total_time_ms_timestamp_is_start_anchored() {
    // Regression (follow-up to #890): `message_nodes.created_at` is
    // stamped when the row is written, which happens once the assistant
    // message (including `metrics`) is finalized, i.e. the turn's *end*,
    // not its start. `total_time_ms` is that turn's elapsed generation
    // time, so sessionize()'s `[timestamp, timestamp + duration_ms]` span
    // would otherwise project forward past the actual completion into
    // phantom idle time. The parser must back-calculate the start anchor
    // instead.
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_session(&conn, "sess-1", "/Users/alice/project", "claude-sonnet-4");
    let chat = r#"{"role":"assistant","content":"hello","metadata":{"generation_model":"claude-sonnet-4","metrics":{"input_tokens":100,"output_tokens":50,"total_time_ms":5000}}}"#;
    insert_message(&conn, "sess-1", chat, 1_700_000_010);
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(
        msg.timestamp,
        1_700_000_010_000 - 5000,
        "timestamp must be back-calculated to the turn start (end - duration)"
    );
    assert_eq!(
        msg.duration_ms,
        Some(5000),
        "duration_ms must still span from start to the recorded end timestamp"
    );
}

#[test]
fn test_parse_devin_cli_sqlite_skips_non_assistant_and_missing_model() {
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_session(&conn, "sess-1", "/Users/alice/project", "glm-5-2-max-1m");
    insert_message(
        &conn,
        "sess-1",
        r#"{"role":"user","content":"hi","metadata":{"metrics":{"input_tokens":1}}}"#,
        1_700_000_000,
    );
    insert_message(
        &conn,
        "sess-1",
        r#"{"role":"assistant","content":"ok","metadata":{"generation_model":"glm-5-2","metrics":{"input_tokens":10,"output_tokens":5}}}"#,
        1_700_000_001,
    );
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn test_parse_devin_cli_sqlite_falls_back_to_session_model() {
    // When generation_model is absent, fall back to sessions.model.
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_session(&conn, "sess-1", "/Users/alice/project", "kimi-k2-7");
    insert_message(
        &conn,
        "sess-1",
        r#"{"role":"assistant","content":"ok","metadata":{"metrics":{"input_tokens":10,"output_tokens":5}}}"#,
        1_700_000_000,
    );
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "kimi-k2-7");
}

#[test]
fn test_parse_devin_cli_sqlite_skips_adaptive_session_model() {
    // When generation_model is absent and sessions.model is "adaptive"
    // (a routing mode), the row should be skipped rather than reported
    // under a fictitious "adaptive" model.
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_session(&conn, "sess-1", "/Users/alice/project", "adaptive");
    insert_message(
        &conn,
        "sess-1",
        r#"{"role":"assistant","metadata":{"metrics":{"input_tokens":10,"output_tokens":5}}}"#,
        1_700_000_000,
    );
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert!(messages.is_empty());
}

#[test]
fn test_parse_devin_cli_sqlite_skips_zero_usage() {
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_session(&conn, "sess-1", "/Users/alice/project", "glm-5-2-max-1m");
    insert_message(
        &conn,
        "sess-1",
        r#"{"role":"assistant","metadata":{"generation_model":"glm-5-2","metrics":{"input_tokens":-100,"output_tokens":-50,"cache_read_tokens":-10,"cache_creation_tokens":-5,"total_time_ms":-1}}}"#,
        1_700_000_000,
    );
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert!(messages.is_empty());
}

#[test]
fn test_parse_devin_cli_sqlite_skips_malformed_rows_without_losing_later_usage() {
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_session(&conn, "sess-1", "/Users/alice/project", "gpt-5");
    insert_message(&conn, "sess-1", "{not valid json", 1_700_000_000);
    insert_message(
        &conn,
        "sess-1",
        r#"{"role":"assistant","metadata":{"metrics":{"input_tokens":10,"output_tokens":5}}}"#,
        1_700_000_001,
    );
    drop(conn);

    let messages = parse_devin_cli_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn test_parse_devin_desktop_ndjson_extracts_usage() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("event.ndjson");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(
            file,
            r#"{{"providerId":"devin-cli","notification":{{"content":{{"text":"hello"}},"metadata":{{"input_tokens":100,"output_tokens":50,"generation_model":"claude-sonnet-4","created_at":"2026-06-16T12:00:00Z"}}}}}}"#
        ).unwrap();
    writeln!(
            file,
            r#"{{"providerId":"devin-cli","notification":{{"content":{{"text":"hi"}},"metadata":{{"input_tokens":0,"output_tokens":0}}}}}}"#
        ).unwrap();
    drop(file);

    let messages = parse_devin_desktop_ndjson(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "devin-desktop");
    assert_eq!(messages[0].model_id, "claude-sonnet-4");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].timestamp, 1_781_611_200_000);
}

#[test]
fn test_parse_devin_desktop_usage_update_without_acp_fields_keeps_legacy_metrics() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("legacy-usage-update.ndjson");
    std::fs::write(
            &path,
            r#"{"notification":{"sessionUpdate":"usage_update","metadata":{"input_tokens":12,"output_tokens":3,"generation_model":"gpt-5"}}}
"#,
        )
        .unwrap();

    let messages = parse_devin_desktop_ndjson(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5");
    assert_eq!(messages[0].tokens.input, 12);
    assert_eq!(messages[0].tokens.output, 3);
}

#[test]
fn test_parse_devin_desktop_ndjson_keeps_distinct_events_with_identical_usage() {
    // Two events with identical model/tokens/timestamp at different line
    // positions must both survive — they represent distinct API calls.
    // The line-index dedup key prevents collision without undercounting.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("event.ndjson");
    std::fs::write(
            &path,
            r#"{"providerId":"devin-cli","notification":{"metadata":{"input_tokens":10,"output_tokens":5,"generation_model":"gpt-5","created_at":"2026-06-16T12:00:00Z"}}}
{"providerId":"devin-cli","notification":{"metadata":{"input_tokens":10,"output_tokens":5,"generation_model":"gpt-5","created_at":"2026-06-16T12:00:00Z"}}}
"#,
        )
        .unwrap();

    let messages = parse_devin_desktop_ndjson(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[1].tokens.input, 10);
}

#[test]
fn test_parse_devin_desktop_acp_usage_aggregates_and_resolves_cli_title() {
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    insert_session(&conn, "cli-session-1", "/Users/alice/project", "gpt-5");
    set_session_title(&conn, "cli-session-1", "Build the release");
    drop(conn);

    let path = dir.path().join("desktop-file-id.ndjson");
    std::fs::write(
            &path,
            concat!(
                r#"{"notification":{"sessionUpdate":"session_info_update","title":"Build the release"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"session_info_update"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":100,"cognition.ai/outputTokens":7,"cognition.ai/cachedReadTokens":20}}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":150,"cognition.ai/outputTokens":8,"cognition.ai/cachedReadTokens":30}}}"#,
                "\n"
            ),
        )
        .unwrap();

    let lookup = load_devin_desktop_session_lookup(std::slice::from_ref(&db_path));
    let messages = parse_devin_desktop_ndjson_with_lookup(&path, &lookup);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "cli-session-1");
    assert_eq!(messages[0].model_id, "gpt-5");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 15);
    assert_eq!(messages[0].tokens.cache_read, 30);
    assert_eq!(messages[0].tokens.total(), 165);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/alice/project")
    );
}

#[test]
fn test_parse_devin_desktop_does_not_resolve_an_ambiguous_title() {
    let dir = TempDir::new().unwrap();
    let db_path = create_devin_cli_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    insert_session(&conn, "cli-session-1", "/Users/alice/project-a", "gpt-5");
    insert_session(
        &conn,
        "cli-session-2",
        "/Users/alice/project-b",
        "claude-sonnet-4",
    );
    set_session_title(&conn, "cli-session-1", "Untitled task");
    set_session_title(&conn, "cli-session-2", "Untitled task");
    drop(conn);

    let path = dir.path().join("desktop-file-id.ndjson");
    std::fs::write(
            &path,
            concat!(
                r#"{"notification":{"sessionUpdate":"session_info_update","title":"Untitled task"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":100,"cognition.ai/outputTokens":7}}}"#,
                "\n"
            ),
        )
        .unwrap();

    let lookup = load_devin_desktop_session_lookup(std::slice::from_ref(&db_path));
    let messages = parse_devin_desktop_ndjson_with_lookup(&path, &lookup);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "desktop-file-id");
    assert_eq!(messages[0].model_id, "devin");
}

#[test]
fn test_parse_devin_cli_sqlite_returns_empty_for_missing_db() {
    let messages = parse_devin_cli_sqlite(Path::new("/nonexistent/devin/sessions.db"));
    assert!(messages.is_empty());
}
