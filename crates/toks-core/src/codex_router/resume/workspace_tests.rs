use std::path::PathBuf;

use rusqlite::Connection;

use super::workspace::{discover_in, validate};
use crate::rotation::ThreadId;

#[test]
fn workspace_comes_from_authoritative_thread_database() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let database = directory.path().join("state_5.sqlite");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT NOT NULL)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO threads (id, cwd) VALUES (?1, ?2)",
            ("thread", workspace.to_str().unwrap()),
        )
        .unwrap();

    assert_eq!(
        validate(discover_in(&database, &ThreadId::new("thread")).unwrap()).unwrap(),
        workspace
    );
}

#[test]
fn workspace_must_be_absolute_and_existing() {
    assert!(validate(PathBuf::from("relative")).is_err());
    assert!(validate(PathBuf::from("/definitely/missing/toks-workspace")).is_err());
}

#[cfg(unix)]
#[test]
fn workspace_symlink_is_resolved_to_a_stable_target() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    let link = directory.path().join("workspace");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    symlink(&first, &link).unwrap();
    let persisted = validate(link.clone()).unwrap();
    assert_eq!(persisted, first);

    std::fs::remove_file(&link).unwrap();
    symlink(&second, &link).unwrap();
    assert_ne!(validate(link).unwrap(), persisted);

    std::fs::remove_dir(&first).unwrap();
    symlink(&second, &first).unwrap();
    assert_ne!(validate(persisted.clone()).unwrap(), persisted);
}
