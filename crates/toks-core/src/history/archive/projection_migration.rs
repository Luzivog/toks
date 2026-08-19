use anyhow::{Context, Result};
use rusqlite::{Connection, TransactionBehavior};

const BACKFILL: &str = include_str!("projection_backfill.sql");

/// Builds the v3 projection from accepted v2 facts with set-based SQL.
///
/// The transaction is all-or-nothing: routine readers see either the previous
/// last-good state or the complete projection, never an undercounted prefix.
pub(super) fn advance(connection: &mut Connection) -> Result<usize> {
    if is_complete(connection)? {
        return Ok(0);
    }
    let pending = pending_count(connection)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(BACKFILL)?;
    transaction
        .commit()
        .context("committing usage projection migration")?;
    Ok(pending)
}

pub(super) fn is_complete(connection: &Connection) -> Result<bool> {
    connection
        .query_row(
            "SELECT complete FROM projection_state WHERE singleton=1",
            [],
            |row| row.get::<_, i64>(0).map(|value| value != 0),
        )
        .map_err(Into::into)
}

pub(super) fn pending_count(connection: &Connection) -> Result<usize> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM events e
         LEFT JOIN projection_events p ON p.event_id=e.event_id
         WHERE p.event_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    usize::try_from(count).context("usage projection backlog does not fit in memory")
}
