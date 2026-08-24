use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, TransactionBehavior};

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i64 = 4;

pub(super) fn open(path: &Path) -> Result<Connection> {
    let parent = path.parent().context("usage archive path has no parent")?;
    fs::create_dir_all(parent)?;
    crate::storage::restrict_directory(parent)?;

    let mut connection = Connection::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    connection.busy_timeout(Duration::from_secs(5))?;

    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        bail!("usage archive was created by a newer Toks version");
    }

    connection.pragma_update(None, "foreign_keys", "ON")?;
    enable_wal(&connection)?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "wal_autocheckpoint", 256)?;
    connection.pragma_update(None, "trusted_schema", "OFF")?;

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(SCHEMA)?;
    if (1..SCHEMA_VERSION).contains(&version) {
        // v4 changes the compact rollup key and pricing basis. Canonical events
        // remain authoritative; only the disposable projection is rebuilt.
        transaction.execute_batch(
            "DROP TABLE IF EXISTS usage_rollups;
             DROP TABLE IF EXISTS projection_events;
             DROP TABLE IF EXISTS projection_state;",
        )?;
        transaction.execute_batch(SCHEMA)?;
    }
    if version < SCHEMA_VERSION {
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    transaction.commit()?;
    Ok(connection)
}

fn enable_wal(connection: &Connection) -> Result<()> {
    const ATTEMPTS: usize = 20;
    for attempt in 0..ATTEMPTS {
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(error) if attempt + 1 == ATTEMPTS => {
                return Err(error).context("enabling usage archive WAL mode");
            }
            Err(_) => thread::sleep(Duration::from_millis(25)),
        }
    }
    unreachable!("WAL retry loop always returns")
}
