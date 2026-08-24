use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::rotation::ThreadId;

pub(super) fn discover(thread: &ThreadId) -> Result<PathBuf> {
    let codex_home = crate::limits::codex::codex_home().context("no Codex home directory")?;
    discover_in(&codex_home.join("state_5.sqlite"), thread)
}

pub(super) fn discover_in(database: &Path, thread: &ThreadId) -> Result<PathBuf> {
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
