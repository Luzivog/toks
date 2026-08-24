use rusqlite::Connection;

use super::thread_source::ThreadSourceStore;
use crate::rotation::ThreadId;

#[test]
fn only_an_authoritative_subagent_row_blocks_external_resume() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("state.sqlite");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, thread_source TEXT)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO threads (id, thread_source) VALUES \
                ('child', 'subagent'), ('root', 'cli')",
            [],
        )
        .unwrap();
    drop(connection);
    let sources = ThreadSourceStore::for_database(database);

    assert!(sources.is_known_subagent(&ThreadId::new("child")));
    assert!(!sources.is_known_subagent(&ThreadId::new("root")));
    assert!(!sources.is_known_subagent(&ThreadId::new("unknown")));
}

#[test]
fn missing_or_older_catalogues_fail_open() {
    let directory = tempfile::tempdir().unwrap();
    let missing = ThreadSourceStore::for_database(directory.path().join("missing.sqlite"));
    assert!(!missing.is_known_subagent(&ThreadId::new("child")));

    let database = directory.path().join("old.sqlite");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute("CREATE TABLE threads (id TEXT PRIMARY KEY)", [])
        .unwrap();
    connection
        .execute("INSERT INTO threads (id) VALUES ('child')", [])
        .unwrap();
    drop(connection);
    let older = ThreadSourceStore::for_database(database);
    assert!(!older.is_known_subagent(&ThreadId::new("child")));
    assert!(!ThreadSourceStore::unavailable().is_known_subagent(&ThreadId::new("child")));
}
