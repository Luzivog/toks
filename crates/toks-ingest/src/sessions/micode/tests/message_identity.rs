use super::*;

#[test]
fn test_parse_micode_sqlite_basic() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");

    let conn = create_micode_sqlite_db(&db_path);

    let data_json = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 100,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0, "completed": 1700000001234.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_001", "ses_001", data_json],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "micode");
    assert_eq!(messages[0].model_id, "mimo-v2.5-pro");
    assert_eq!(messages[0].provider_id, "mimo");
    assert_eq!(messages[0].tokens.input, 1000);
    assert_eq!(messages[0].tokens.output, 500);
    assert_eq!(messages[0].tokens.reasoning, 100);
    assert_eq!(messages[0].tokens.cache_read, 200);
    assert_eq!(messages[0].tokens.cache_write, 50);
    assert!((messages[0].cost - 0.05).abs() < 1e-9);
    assert_eq!(
        messages[0].cost_source,
        super::super::super::CostSource::ProviderReported
    );
    assert_eq!(messages[0].duration_ms, Some(1234));
}

#[test]
fn test_parse_micode_sqlite_skips_user_messages() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");

    let conn = create_micode_sqlite_db(&db_path);

    let user_msg = r#"{
        "role": "user",
        "modelID": "mimo-v2.5-pro",
        "time": { "created": 1700000000000.0 }
    }"#;

    let assistant_msg = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "tokens": { "input": 100, "output": 50, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000001000.0 }
    }"#;

    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_user", "ses_001", user_msg],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["msg_assistant", "ses_001", assistant_msg],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    // This message carries no embedded JSON id, so the dedup key falls back
    // to the SQLite row id and is namespaced by the database path.
    assert!(messages[0]
        .dedup_key
        .as_deref()
        .is_some_and(|key| key.ends_with(":msg_assistant")));
}

/// Regression: MiMo Code uses channel-suffixed databases (mimocode.db and
/// mimocode-<channel>.db). A mid-session channel switch can write the SAME
/// message (same embedded id) to both files. The embedded id must NOT be
/// namespaced by the database, otherwise the cross-file dedup set produces
/// two distinct keys and the message's cost + tokens get counted twice.
#[test]
fn embedded_message_id_is_not_namespaced_by_database() {
    let dir = tempfile::tempdir().unwrap();
    let db_a = dir.path().join("mimocode.db");
    let db_b = dir.path().join("mimocode-beta.db");
    // Embedded JSON "id" is the globally unique message id.
    let msg = r#"{
        "id": "msg_shared",
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": { "input": 10, "output": 5 },
        "time": { "created": 1700000000000.0 }
    }"#;
    // Different SQLite row ids prove the collapse is driven by the embedded
    // id (not the row id), exactly as a mid-session channel switch records.
    for (db, row_id) in [(&db_a, "row_a"), (&db_b, "row_b")] {
        let conn = create_micode_sqlite_db(db);
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![row_id, "ses_1", msg],
        )
        .unwrap();
        drop(conn);
    }

    let a = parse_micode_sqlite(&db_a);
    let b = parse_micode_sqlite(&db_b);
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    // Same embedded id across both channel databases yields IDENTICAL,
    // un-namespaced dedup keys, so a shared dedup set collapses the
    // duplicate to a single count.
    assert_eq!(a[0].dedup_key, Some("msg_shared".to_string()));
    assert_eq!(b[0].dedup_key, Some("msg_shared".to_string()));

    // Prove the collapse end-to-end with the same HashSet logic used by the
    // cross-file aggregation in lib.rs.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let kept: Vec<_> = a
        .into_iter()
        .chain(b)
        .filter(|m| m.dedup_key.as_ref().is_none_or(|k| seen.insert(k.clone())))
        .collect();
    assert_eq!(kept.len(), 1, "shared embedded id must be counted once");
}

/// Two DIFFERENT messages that happen to share a SQLite rowid across two
/// databases (rowids are per-database, not globally unique) must NOT be
/// collapsed by the cross-file dedup set. The row-id fallback path is
/// namespaced by database precisely to keep them distinct.
#[test]
fn rowid_fallback_is_namespaced_by_database() {
    let dir = tempfile::tempdir().unwrap();
    let db_a = dir.path().join("a.db");
    let db_b = dir.path().join("b.db");
    // No embedded "id" field -> the parser falls back to the SQLite rowid.
    let msg = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": { "input": 10, "output": 5 },
        "time": { "created": 1700000000000.0 }
    }"#;
    for db in [&db_a, &db_b] {
        let conn = create_micode_sqlite_db(db);
        // Same SQLite row id ("id" column) in both databases. With no
        // embedded JSON id, the parser falls back to this row id.
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params!["row_shared", "ses_1", msg],
        )
        .unwrap();
        drop(conn);
    }

    let a = parse_micode_sqlite(&db_a);
    let b = parse_micode_sqlite(&db_b);
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    // Same row id ("row_shared") in two databases must yield DISTINCT,
    // db-namespaced keys so the two unrelated messages are not merged.
    assert_ne!(a[0].dedup_key, b[0].dedup_key);
    assert!(a[0].dedup_key.as_deref().unwrap().ends_with(":row_shared"));
    assert!(b[0].dedup_key.as_deref().unwrap().ends_with(":row_shared"));

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let kept: Vec<_> = a
        .into_iter()
        .chain(b)
        .filter(|m| m.dedup_key.as_ref().is_none_or(|k| seen.insert(k.clone())))
        .collect();
    assert_eq!(
        kept.len(),
        2,
        "rowid collisions across DBs must stay distinct"
    );
}
