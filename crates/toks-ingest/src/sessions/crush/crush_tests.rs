use super::*;
use rusqlite::params;
use tempfile::TempDir;

fn create_test_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("crush.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                parent_session_id TEXT,
                title TEXT,
                message_count INTEGER NOT NULL DEFAULT 0,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                cost REAL NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                parts TEXT NOT NULL DEFAULT '[]',
                model TEXT,
                provider TEXT,
                is_summary_message INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                finished_at INTEGER
            );
            "#,
    )
    .unwrap();
    db_path
}

fn insert_root_session(
    conn: &Connection,
    id: &str,
    message_count: i64,
    cost: f64,
    updated_at: i64,
    created_at: i64,
) {
    conn.execute(
            "INSERT INTO sessions (id, parent_session_id, title, message_count, cost, updated_at, created_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6)",
            params![id, "Root", message_count, cost, updated_at, created_at],
        )
        .unwrap();
}

fn insert_child_session(conn: &Connection, id: &str, parent_id: &str) {
    conn.execute(
            "INSERT INTO sessions (id, parent_session_id, title, message_count, cost, updated_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, parent_id, "Child", 2_i64, 99.0_f64, 1_742_342_001_i64, 1_742_300_100_i64],
        )
        .unwrap();
}

fn insert_message(
    conn: &Connection,
    id: &str,
    session_id: &str,
    role: &str,
    created_at: i64,
    is_summary_message: i64,
) {
    conn.execute(
            "INSERT INTO messages (id, session_id, role, parts, model, provider, is_summary_message, created_at, updated_at)
             VALUES (?1, ?2, ?3, '[]', 'gpt-5.4', 'crush', ?4, ?5, ?5)",
            params![id, session_id, role, is_summary_message, created_at],
        )
        .unwrap();
}

/// Crush is the one parser whose day split is decided at parse time: it
/// allocates a session's cost across the days its assistant turns fall in
/// and emits one message per day. The post-parse rebucket pass can relabel
/// those messages but cannot recover a split this grouping collapsed, so
/// the pinned zone has to reach in here.
///
/// Two turns 6.5 hours apart: one calendar day in Los Angeles, two in
/// Kiritimati. The cost split follows whichever zone is pinned.
#[test]
fn test_parse_crush_sqlite_splits_cost_by_the_pinned_timezone() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // 2026-03-02T11:30:00Z and 2026-03-02T18:00:00Z.
    let first = 1_772_451_000_i64;
    let second = 1_772_474_400_i64;

    insert_root_session(&conn, "root-tz", 2, 10.0, second, first);
    insert_message(&conn, "msg-1", "root-tz", "assistant", first, 0);
    insert_message(&conn, "msg-2", "root-tz", "assistant", second, 0);

    // Los Angeles: 03:30 and 10:00 on 2026-03-02 — one day, one message.
    let los_angeles = parse_crush_sqlite_in(
        &db_path,
        &BucketTimezone::from_pinned_name(Some("America/Los_Angeles")),
    );
    assert_eq!(los_angeles.len(), 1);
    assert_eq!(los_angeles[0].message_count, 2);
    assert!((los_angeles[0].cost - 10.0).abs() < 1e-9);

    // Kiritimati: 2026-03-03 01:30 and 08:00 — still one day, because both
    // turns cross together. Seoul is where they separate: 20:30 on the 2nd
    // and 03:00 on the 3rd.
    let seoul = parse_crush_sqlite_in(
        &db_path,
        &BucketTimezone::from_pinned_name(Some("Asia/Seoul")),
    );
    assert_eq!(seoul.len(), 2, "Seoul splits the session across two days");
    assert_eq!(seoul[0].message_count, 1);
    assert_eq!(seoul[1].message_count, 1);
    assert!(
        (seoul.iter().map(|m| m.cost).sum::<f64>() - 10.0).abs() < 1e-9,
        "splitting must conserve the session cost"
    );

    // And the split is a function of the pin alone: the same call twice
    // returns the same shape regardless of what the host machine is set to.
    let seoul_again = parse_crush_sqlite_in(
        &db_path,
        &BucketTimezone::from_pinned_name(Some("Asia/Seoul")),
    );
    assert_eq!(
        seoul.iter().map(|m| m.timestamp).collect::<Vec<_>>(),
        seoul_again.iter().map(|m| m.timestamp).collect::<Vec<_>>()
    );
}

#[test]
fn test_parse_crush_sqlite_allocates_cost_across_assistant_message_days() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    let day_one = 1_742_300_000_i64;
    let day_two = 1_742_386_400_i64;

    insert_root_session(&conn, "root-1", 5, 30.0, day_two, day_one);
    insert_child_session(&conn, "child-1", "root-1");
    insert_message(&conn, "msg-1", "root-1", "assistant", day_one, 0);
    insert_message(&conn, "msg-2", "root-1", "user", day_one + 10, 0);
    insert_message(&conn, "msg-3", "root-1", "assistant", day_two, 0);
    insert_message(&conn, "msg-4", "root-1", "assistant", day_two + 10, 1);
    insert_message(&conn, "msg-5", "child-1", "assistant", day_two + 20, 0);

    let messages = parse_crush_sqlite(&db_path);
    assert_eq!(messages.len(), 2);

    assert_eq!(messages[0].client, "crush");
    assert_eq!(messages[0].model_id, CRUSH_MODEL_ID);
    assert_eq!(messages[0].provider_id, CRUSH_PROVIDER_ID);
    assert_eq!(messages[0].timestamp, day_one * 1000);
    assert_eq!(messages[0].message_count, 1);
    assert!((messages[0].cost - 7.5).abs() < 1e-9);

    assert_eq!(messages[1].timestamp, day_two * 1000);
    assert_eq!(messages[1].message_count, 3);
    assert!((messages[1].cost - 22.5).abs() < 1e-9);
    assert!(
        (messages.iter().map(|msg| msg.cost).sum::<f64>() - 30.0).abs() < 1e-9,
        "allocated cost must sum back to the stored session total"
    );
    assert!(messages
        .iter()
        .all(|msg| msg.session_id.ends_with(":root-1")));
    assert!(messages.iter().all(|msg| msg.tokens.total() == 0));
}

#[test]
fn test_parse_crush_sqlite_uses_updated_at_when_costed_session_has_no_assistant_messages() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_root_session(
        &conn,
        "root-1",
        3,
        4.5,
        1_742_342_000_i64,
        1_742_300_000_i64,
    );
    insert_message(&conn, "msg-1", "root-1", "user", 1_742_300_100_i64, 0);

    let messages = parse_crush_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, 1_742_342_000_000_i64);
    assert_eq!(messages[0].message_count, 0);
    assert_eq!(messages[0].cost, 4.5);
}

#[test]
fn test_parse_crush_sqlite_preserves_millisecond_timestamps() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    let created_at_ms = 1_742_300_000_123_i64;
    insert_root_session(&conn, "root-1", 1, 2.0, created_at_ms, created_at_ms);
    insert_message(&conn, "msg-1", "root-1", "assistant", created_at_ms, 0);

    let messages = parse_crush_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, created_at_ms);
    assert_eq!(messages[0].message_count, 1);
    assert_eq!(messages[0].cost, 2.0);
}

#[test]
fn test_parse_crush_sqlite_includes_child_session_assistant_messages() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    let day_one = 1_742_300_000_i64;
    let day_two = 1_742_386_400_i64;

    insert_root_session(&conn, "root-1", 4, 40.0, day_two, day_one);
    insert_child_session(&conn, "child-1", "root-1");
    insert_message(&conn, "msg-1", "root-1", "assistant", day_one, 0);
    insert_message(&conn, "msg-2", "child-1", "assistant", day_two, 0);

    let messages = parse_crush_sqlite(&db_path);
    assert_eq!(
            messages.len(),
            2,
            "root-session cost should be distributed across assistant messages in descendant sessions too"
        );
    assert_eq!(messages[0].timestamp, day_one * 1000);
    assert_eq!(messages[0].message_count, 1);
    assert!((messages[0].cost - 20.0).abs() < 1e-9);

    assert_eq!(messages[1].timestamp, day_two * 1000);
    assert_eq!(messages[1].message_count, 1);
    assert!((messages[1].cost - 20.0).abs() < 1e-9);
    assert!(messages
        .iter()
        .all(|msg| msg.session_id.ends_with(":root-1")));
}

#[test]
fn test_parse_crush_sqlite_returns_empty_for_missing_db() {
    let messages = parse_crush_sqlite(Path::new("/nonexistent/crush.db"));
    assert!(messages.is_empty());
}

#[test]
fn test_parse_crush_sqlite_skips_sessions_without_valid_timestamps() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    insert_root_session(&conn, "root-1", 3, 4.5, 0, 0);

    let messages = parse_crush_sqlite(&db_path);
    assert!(messages.is_empty());
}
