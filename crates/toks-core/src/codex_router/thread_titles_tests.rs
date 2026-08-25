use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use rusqlite::{params, Connection};

use super::thread_titles::ThreadTitleStore;
use crate::rotation::ThreadId;

fn create_database(home: &Path) -> Connection {
    let connection = Connection::open(home.join("state_5.sqlite")).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (\
                id TEXT PRIMARY KEY, \
                name TEXT, \
                title TEXT, \
                preview TEXT\
            )",
            [],
        )
        .unwrap();
    connection
}

fn insert_thread(
    connection: &Connection,
    id: &str,
    name: Option<&str>,
    title: &str,
    preview: &str,
) {
    connection
        .execute(
            "INSERT INTO threads (id, name, title, preview) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, title, preview],
        )
        .unwrap();
}

fn index_line(id: &str, name: &str, updated_at: &str) -> String {
    serde_json::json!({
        "id": id,
        "thread_name": name,
        "updated_at": updated_at,
    })
    .to_string()
}

fn title(store: &ThreadTitleStore, id: &str) -> Option<String> {
    store
        .titles(&[ThreadId::new(id)])
        .remove(&ThreadId::new(id))
}

#[test]
fn explicit_database_name_wins_and_whitespace_is_collapsed() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path());
    insert_thread(
        &connection,
        "named",
        Some("  Assigned\n\ttitle  "),
        "Stale derived title",
        "Stale preview",
    );
    fs::write(
        home.path().join("session_index.jsonl"),
        format!(
            "{}\n",
            index_line("named", "Newer index title", "2026-08-25T10:00:00Z")
        ),
    )
    .unwrap();

    assert_eq!(
        title(&ThreadTitleStore::new(home.path().to_path_buf()), "named"),
        Some("Assigned title".to_owned())
    );
}

#[test]
fn session_index_rename_beats_a_name_less_database_title() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path());
    insert_thread(
        &connection,
        "renamed",
        Some(" \n "),
        "First user message",
        "First user message",
    );
    fs::write(
        home.path().join("session_index.jsonl"),
        format!(
            "{}\n",
            index_line("renamed", "  Project\nplan  ", "2026-08-25T10:00:00Z")
        ),
    )
    .unwrap();

    assert_eq!(
        title(&ThreadTitleStore::new(home.path().to_path_buf()), "renamed"),
        Some("Project plan".to_owned())
    );
}

#[test]
fn changed_index_is_reparsed_and_the_last_valid_entry_wins() {
    let home = tempfile::tempdir().unwrap();
    let index = home.path().join("session_index.jsonl");
    fs::write(
        &index,
        format!(
            "{}\n",
            index_line("changing", "Old name", "2026-08-25T10:00:00Z")
        ),
    )
    .unwrap();
    let store = ThreadTitleStore::new(home.path().to_path_buf());
    assert_eq!(title(&store, "changing"), Some("Old name".to_owned()));

    let mut file = OpenOptions::new().append(true).open(index).unwrap();
    writeln!(file, "not valid json").unwrap();
    writeln!(
        file,
        "{}",
        index_line("changing", "Latest name", "2026-08-25T10:01:00Z")
    )
    .unwrap();
    drop(file);

    assert_eq!(title(&store, "changing"), Some("Latest name".to_owned()));
}

#[test]
fn derived_title_is_collapsed_and_truncated_to_eighty_characters() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path());
    let prompt = format!("  First\n\tsecond   {}  ", "x".repeat(100));
    insert_thread(&connection, "long", None, &prompt, "Unused preview");

    let resolved = title(&ThreadTitleStore::new(home.path().to_path_buf()), "long").unwrap();
    assert_eq!(resolved, format!("First second {}…", "x".repeat(66)));
    assert_eq!(resolved.chars().count(), 80);
}

#[test]
fn blank_title_uses_preview() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path());
    insert_thread(&connection, "previewed", None, " \n ", "  Preview\n text  ");

    assert_eq!(
        title(
            &ThreadTitleStore::new(home.path().to_path_buf()),
            "previewed"
        ),
        Some("Preview text".to_owned())
    );
}

#[test]
fn missing_database_and_unknown_id_have_no_title() {
    let missing_home = tempfile::tempdir().unwrap();
    let missing = ThreadTitleStore::new(missing_home.path().to_path_buf());
    assert_eq!(title(&missing, "missing"), None);

    let home = tempfile::tempdir().unwrap();
    let _connection = create_database(home.path());
    let store = ThreadTitleStore::new(home.path().to_path_buf());
    assert_eq!(title(&store, "unknown"), None);
}

#[test]
fn missing_database_column_degrades_to_no_title() {
    let home = tempfile::tempdir().unwrap();
    let connection = Connection::open(home.path().join("state_5.sqlite")).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, preview TEXT)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO threads (id, title, preview) VALUES ('legacy', 'Title', 'Preview')",
            [],
        )
        .unwrap();
    drop(connection);

    assert_eq!(
        title(&ThreadTitleStore::new(home.path().to_path_buf()), "legacy"),
        None
    );
}

#[test]
fn missing_database_table_degrades_to_no_title() {
    let home = tempfile::tempdir().unwrap();
    let connection = Connection::open(home.path().join("state_5.sqlite")).unwrap();
    drop(connection);

    assert_eq!(
        title(
            &ThreadTitleStore::new(home.path().to_path_buf()),
            "missing-table"
        ),
        None
    );
}

#[test]
fn locked_database_falls_back_to_the_session_index() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path());
    insert_thread(
        &connection,
        "locked",
        Some("Database title"),
        "Derived title",
        "Preview",
    );
    fs::write(
        home.path().join("session_index.jsonl"),
        format!(
            "{}\n",
            index_line("locked", "Index title", "2026-08-25T10:00:00Z")
        ),
    )
    .unwrap();
    connection.execute_batch("BEGIN EXCLUSIVE").unwrap();

    assert_eq!(
        title(&ThreadTitleStore::new(home.path().to_path_buf()), "locked"),
        Some("Index title".to_owned())
    );
}
