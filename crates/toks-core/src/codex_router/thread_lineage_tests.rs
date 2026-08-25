use std::path::Path;

use rusqlite::{params, Connection};

use super::thread_lineage::{ThreadLineage, ThreadLineageKind, ThreadLineageStore};
use crate::rotation::ThreadId;

fn create_database(home: &Path, with_edges: bool) -> Connection {
    let connection = Connection::open(home.join("state_5.sqlite")).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (\
                id TEXT PRIMARY KEY, \
                thread_source TEXT, \
                source TEXT, \
                agent_nickname TEXT\
            )",
            [],
        )
        .unwrap();
    if with_edges {
        connection
            .execute(
                "CREATE TABLE thread_spawn_edges (\
                    parent_thread_id TEXT, \
                    child_thread_id TEXT PRIMARY KEY, \
                    status TEXT\
                )",
                [],
            )
            .unwrap();
    }
    connection
}

fn insert_thread(
    connection: &Connection,
    id: &str,
    thread_source: Option<&str>,
    source: Option<&str>,
    nickname: Option<&str>,
) {
    connection
        .execute(
            "INSERT INTO threads (id, thread_source, source, agent_nickname) \
             VALUES (?1, ?2, ?3, ?4)",
            params![id, thread_source, source, nickname],
        )
        .unwrap();
}

fn source(parent: &str) -> String {
    serde_json::json!({
        "subagent": {
            "thread_spawn": {
                "parent_thread_id": parent,
                "depth": 1
            }
        }
    })
    .to_string()
}

fn read(home: &Path, ids: &[&str]) -> std::collections::BTreeMap<ThreadId, ThreadLineage> {
    ThreadLineageStore::new(home.to_path_buf())
        .lineages(&ids.iter().map(|id| ThreadId::new(*id)).collect::<Vec<_>>())
}

#[test]
fn spawn_edge_wins_over_source_json_and_closed_edges_are_kept() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path(), true);
    insert_thread(
        &connection,
        "child",
        Some("subagent"),
        Some(&source("source-parent")),
        Some(" Dirac "),
    );
    connection
        .execute(
            "INSERT INTO thread_spawn_edges VALUES ('edge-parent', 'child', 'closed')",
            [],
        )
        .unwrap();

    assert_eq!(
        read(home.path(), &["child"])[&ThreadId::new("child")],
        ThreadLineage {
            kind: ThreadLineageKind::Subagent {
                parent: Some(ThreadId::new("edge-parent")),
            },
            agent_nickname: Some("Dirac".into()),
        }
    );
}

#[test]
fn source_json_supplies_parent_when_the_edge_row_is_absent() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path(), true);
    insert_thread(
        &connection,
        "child",
        Some("subagent"),
        Some(&source("source-parent")),
        None,
    );

    assert_eq!(
        read(home.path(), &["child"])[&ThreadId::new("child")].kind,
        ThreadLineageKind::Subagent {
            parent: Some(ThreadId::new("source-parent")),
        }
    );
}

#[test]
fn user_rows_are_top_level_and_missing_classification_stays_unknown() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path(), true);
    insert_thread(&connection, "user", Some("user"), None, None);
    insert_thread(&connection, "unknown", None, None, Some("Noether"));

    let lineage = read(home.path(), &["user", "unknown", "missing"]);
    assert_eq!(
        lineage[&ThreadId::new("user")].kind,
        ThreadLineageKind::TopLevel
    );
    assert!(!lineage.contains_key(&ThreadId::new("unknown")));
    assert!(!lineage.contains_key(&ThreadId::new("missing")));
}

#[test]
fn missing_spawn_edge_table_falls_back_to_source_json() {
    let home = tempfile::tempdir().unwrap();
    let connection = create_database(home.path(), false);
    insert_thread(
        &connection,
        "legacy-child",
        Some("subagent"),
        Some(&source("legacy-parent")),
        Some("Emmy"),
    );

    assert_eq!(
        read(home.path(), &["legacy-child"])[&ThreadId::new("legacy-child")],
        ThreadLineage {
            kind: ThreadLineageKind::Subagent {
                parent: Some(ThreadId::new("legacy-parent")),
            },
            agent_nickname: Some("Emmy".into()),
        }
    );
}

#[test]
fn missing_threads_table_returns_no_lineage() {
    let home = tempfile::tempdir().unwrap();
    Connection::open(home.path().join("state_5.sqlite")).unwrap();

    assert!(read(home.path(), &["child"]).is_empty());
}
