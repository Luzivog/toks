use super::*;

#[test]
fn test_parse_micode_sqlite_dedup_forked_history() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    let conn = create_micode_sqlite_db(&db_path);

    let root_msg = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.05,
        "tokens": {
            "input": 1000,
            "output": 500,
            "reasoning": 25,
            "cache": { "read": 200, "write": 50 }
        },
        "time": { "created": 1700000000000.0, "completed": 1700000000500.0 }
    }"#;

    let new_msg = r#"{
        "role": "assistant",
        "modelID": "mimo-v2.5-pro",
        "providerID": "mimo",
        "cost": 0.08,
        "tokens": {
            "input": 1300,
            "output": 650,
            "reasoning": 40,
            "cache": { "read": 100, "write": 0 }
        },
        "time": { "created": 1700000001000.0, "completed": 1700000001500.0 }
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

    let messages = parse_micode_sqlite(&db_path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 1000);
    assert_eq!(messages[1].tokens.input, 1300);
}

#[test]
fn duplicate_explicit_zero_cost_upgrades_retained_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_micode.db");
    let conn = create_micode_sqlite_db(&db_path);
    let without_cost = r#"{
        "role": "assistant",
        "modelID": "unknown-model",
        "providerID": "mimo",
        "tokens": { "input": 10, "output": 5 },
        "time": { "created": 1700000000000.0 }
    }"#;
    let with_zero_cost = r#"{
        "role": "assistant",
        "modelID": "unknown-model",
        "providerID": "mimo",
        "cost": 0,
        "tokens": { "input": 10, "output": 5 },
        "time": { "created": 1700000000000.0 }
    }"#;
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["row_a", "session_a", without_cost],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
        rusqlite::params!["row_b", "session_b", with_zero_cost],
    )
    .unwrap();
    drop(conn);

    let messages = parse_micode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].cost, 0.0);
    assert_eq!(
        messages[0].cost_source,
        super::super::super::CostSource::ProviderReported
    );
}
