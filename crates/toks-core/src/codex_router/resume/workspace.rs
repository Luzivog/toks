use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::rotation::ThreadId;

pub(super) fn discover(thread: &ThreadId) -> Result<PathBuf> {
    let codex_home = crate::limits::codex::codex_home().context("no Codex home directory")?;
    discover_in(&codex_home.join("state_5.sqlite"), thread)
}

fn discover_in(database: &Path, thread: &ThreadId) -> Result<PathBuf> {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("opening Codex thread state")?;
    let cwd = connection
        .query_row(
            "SELECT cwd FROM threads WHERE id = ?1 LIMIT 1",
            [thread.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("reading Codex thread workspace")?
        .context("Codex thread workspace was not found")?;
    Ok(PathBuf::from(cwd))
}

pub(super) fn validate(cwd: PathBuf) -> Result<PathBuf> {
    anyhow::ensure!(cwd.is_absolute(), "Codex thread workspace is not absolute");
    let canonical = cwd
        .canonicalize()
        .context("canonicalizing Codex thread workspace")?;
    anyhow::ensure!(
        canonical.is_dir(),
        "Codex thread workspace is not a directory"
    );
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;

    use super::{discover_in, validate};
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
}
