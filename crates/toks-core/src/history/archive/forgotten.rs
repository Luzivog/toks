use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::{params, Connection, TransactionBehavior};
use toks_ingest::sessions::UnifiedMessage;

use super::{projection_migration, schema};

pub(super) fn allowed(
    connection: &Connection,
    observations: &[UnifiedMessage],
) -> Result<Vec<UnifiedMessage>> {
    let mut statement = connection.prepare("SELECT start_ms, end_ms FROM forgotten_ranges")?;
    let ranges = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(observations
        .iter()
        .filter(|message| {
            ranges
                .iter()
                .all(|(start, end)| message.timestamp < *start || message.timestamp >= *end)
        })
        .cloned()
        .collect())
}

pub(super) fn forget_range(path: &Path, start_ms: i64, end_ms: i64) -> Result<usize> {
    if end_ms <= start_ms {
        bail!("forgotten usage range must end after it starts");
    }
    let mut connection = schema::open(path)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT OR IGNORE INTO forgotten_ranges VALUES (?1, ?2)",
        params![start_ms, end_ms],
    )?;
    let removed = transaction.execute(
        "DELETE FROM events WHERE timestamp_ms>=?1 AND timestamp_ms<?2",
        params![start_ms, end_ms],
    )?;
    if removed > 0 {
        transaction.execute_batch(
            "DELETE FROM usage_rollups;
             DELETE FROM projection_events;
             UPDATE projection_state SET
               complete=0, strong_events=0, weak_events=0, conflicts=0
             WHERE singleton=1;",
        )?;
    }
    transaction.commit()?;
    if removed > 0 {
        projection_migration::advance(&mut connection)?;
    }
    Ok(removed)
}
