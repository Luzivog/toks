use super::*;

#[test]
fn test_parse_micode_sqlite_negative_values_clamped() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");

    let conn = create_micode_sqlite_db(&db_path);

    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": -0.05,
        "tokens": {
            "input": -100,
            "output": -50,
            "reasoning": -25,
            "cache": { "read": -200, "write": -10 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_negative", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 0);
    assert_eq!(messages[0].tokens.output, 0);
    assert_eq!(messages[0].tokens.cache_read, 0);
    assert_eq!(messages[0].tokens.cache_write, 0);
    assert_eq!(messages[0].tokens.reasoning, 0);
    assert!(messages[0].cost >= 0.0);
}

#[test]
fn test_parse_micode_sqlite_workspace_from_session() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    let conn = create_micode_sqlite_db(&db_path);
    conn.execute_batch(
        "CREATE TABLE session (
            id TEXT PRIMARY KEY,
            directory TEXT NOT NULL
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, directory) VALUES (?1, ?2)",
        rusqlite::params!["ses_001", "/Users/alice/micode-repo"],
    )
    .unwrap();

    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
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
        rusqlite::params!["msg_ws", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/alice/micode-repo")
    );
    assert_eq!(messages[0].workspace_label.as_deref(), Some("micode-repo"));
}

#[test]
fn test_parse_micode_sqlite_with_agent() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    let conn = create_micode_sqlite_db(&db_path);

    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "agent": "build",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 100,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_agent", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent, Some("Build".to_string()));
}

/// Regression for PR #710: `time.created` was hard-assumed to be epoch
/// milliseconds. If MiMo writes epoch *seconds*, the date landed ~1000x in
/// the past (1970-era). A ms-valued and a seconds-valued `time.created` that
/// denote the SAME instant must normalize to the same date and the same
/// (millisecond-scale) timestamp. Without `micode_timestamp_to_ms`, the
/// seconds variant would yield 1970-01-20 instead of 2023-11-14.
#[test]
fn test_parse_micode_sqlite_normalizes_seconds_and_milliseconds() {
    let dir = tempfile::tempdir().unwrap();
    let db_ms = dir.path().join("ms.db");
    let db_secs = dir.path().join("secs.db");

    // 1_700_000_000 s == 1_700_000_000_000 ms == 2023-11-14T22:13:20Z.
    let msg_ms = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": { "input": 10, "output": 5 },
        "time": { "created": 1700000000000.0, "completed": 1700000001234.0 }
    }"#;
    // Same instant, expressed in epoch SECONDS (the bugged-input shape).
    let msg_secs = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": { "input": 10, "output": 5 },
        "time": { "created": 1700000000.0, "completed": 1700000001.234 }
    }"#;

    for (db, data) in [(&db_ms, msg_ms), (&db_secs, msg_secs)] {
        let conn = create_micode_sqlite_db(db);
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params!["msg_1", "ses_1", data],
        )
        .unwrap();
        drop(conn);
    }

    let ms = parse_micode_sqlite(&db_ms);
    let secs = parse_micode_sqlite(&db_secs);
    assert_eq!(ms.len(), 1);
    assert_eq!(secs.len(), 1);

    // Both inputs resolve to the SAME instant: identical timestamp (ms) and
    // identical, non-empty (i.e. not 1970-era-then-formatted) date.
    assert_eq!(ms[0].timestamp, 1_700_000_000_000);
    assert_eq!(secs[0].timestamp, 1_700_000_000_000);
    assert_eq!(ms[0].date, secs[0].date);
    assert!(!ms[0].date.is_empty());

    // Duration is in milliseconds for BOTH representations (~1234 ms), not
    // ~1 (which is what the seconds input would have produced unnormalized).
    assert_eq!(ms[0].duration_ms, Some(1234));
    assert_eq!(secs[0].duration_ms, Some(1234));
}

/// A non-object `path` field (e.g. a bare string instead of `{ "root": .. }`)
/// must not crash deserialization or fail the whole message: the custom
/// `deserialize_micode_path` extracts `root` defensively, leaving it `None`.
/// The message must still parse and have no embedded-path workspace.
#[test]
fn test_parse_micode_sqlite_non_object_path_field() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    let conn = create_micode_sqlite_db(&db_path);

    // `path` is a string, not an object — the deserializer's `.get("root")`
    // returns None rather than erroring, so the message survives.
    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": { "input": 100, "output": 50 },
        "path": "/some/string/not/an/object",
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_badpath", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(
        messages.len(),
        1,
        "non-object path must not drop the message"
    );
    assert_eq!(messages[0].tokens.input, 100);
    // No usable root -> no workspace derived from the embedded path.
    assert_eq!(messages[0].workspace_key, None);
    assert_eq!(messages[0].workspace_label, None);
}

/// Legacy-query fallback: when the database has no `session` table, the
/// modern query (which JOINs `session`) fails to prepare and the parser
/// falls back to `legacy_query`. In that path `workspace_root` from the row
/// is NULL, so the workspace must come from the message's EMBEDDED `path.root`.
#[test]
fn test_parse_micode_sqlite_legacy_fallback_embedded_path_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    // Note: create_micode_sqlite_db creates ONLY the `message` table, so the
    // modern query's `LEFT JOIN session` cannot prepare and we exercise the
    // legacy fallback.
    let conn = create_micode_sqlite_db(&db_path);

    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": { "input": 100, "output": 50 },
        "path": { "root": "/Users/bob/embedded-repo" },
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_embedded", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    // Row workspace_root is NULL on the legacy path, so the embedded
    // `path.root` supplies the workspace.
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("/Users/bob/embedded-repo")
    );
    assert_eq!(
        messages[0].workspace_label.as_deref(),
        Some("embedded-repo")
    );
}

#[test]
fn test_parse_micode_sqlite_missing_cache_defaults_to_zero() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    let conn = create_micode_sqlite_db(&db_path);

    // Assistant payload with no `cache` object at all — must parse (not be
    // dropped) with cache tokens defaulting to 0.
    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 100
        },
        "time": { "created": 1700000000000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_no_cache", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 1000);
    assert_eq!(messages[0].tokens.output, 500);
    assert_eq!(messages[0].tokens.cache_read, 0);
    assert_eq!(messages[0].tokens.cache_write, 0);
}
