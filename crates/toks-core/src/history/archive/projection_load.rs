use anyhow::Result;
use rusqlite::{Connection, Row};

use super::projection_migration;
use super::{checkpoint, ArchiveProjection, ArchiveRollup, RollupPeriod};

/// Visits compact rollups one at a time. The caller can derive timezone-aware
/// UI buckets without retaining one value per lifetime minute.
pub(super) fn stream(
    connection: &Connection,
    mut visit: impl FnMut(&ArchiveRollup),
) -> Result<ArchiveProjection> {
    let mut statement = connection.prepare(
        "SELECT period, bucket_start_ms, client, provider, model,
         cost_source, long_context, input_tokens, output_tokens, cache_read_tokens,
         cache_write_tokens, reasoning_tokens, message_count, turn_count,
         cost_nanos, event_count,
         input_b0, input_b1, input_b2, input_b3, input_b4,
         output_b0, output_b1, output_b2, output_b3, output_b4,
         cache_read_b0, cache_read_b1, cache_read_b2,
         cache_write_b0, cache_write_b1
         FROM usage_rollups WHERE period IN (0, 1)
         ORDER BY period, bucket_start_ms, client, provider, model, cost_source",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        visit(&bucket_from_row(row)?);
    }
    drop(rows);
    drop(statement);
    metadata(connection)
}

pub(super) fn metadata(connection: &Connection) -> Result<ArchiveProjection> {
    let (captured_since_ms, captured_through_ms) = connection
        .query_row(
            "SELECT captured_since_ms, captured_through_ms FROM archive_state WHERE singleton=1",
            [],
            |row| Ok((Some(row.get(0)?), Some(row.get(1)?))),
        )
        .unwrap_or((None, None));
    let (strong_events, weak_events, conflicts) = connection.query_row(
        "SELECT strong_events, weak_events, conflicts FROM projection_state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let projection_complete = projection_migration::is_complete(connection)?;
    let projection_pending = if projection_complete {
        0
    } else {
        projection_migration::pending_count(connection)?
    };
    Ok(ArchiveProjection {
        #[cfg(test)]
        rollups: Vec::new(),
        captured_since_ms,
        captured_through_ms,
        pending_sources: checkpoint::pending_count(connection)?,
        projection_pending,
        projection_complete,
        strong_events,
        weak_events,
        conflicts,
    })
}

#[cfg(test)]
pub(super) fn load(connection: &Connection) -> Result<ArchiveProjection> {
    let mut rollups = Vec::new();
    let mut projection = stream(connection, |row| rollups.push(row.clone()))?;
    projection.rollups = rollups;
    Ok(projection)
}

fn bucket_from_row(row: &Row<'_>) -> rusqlite::Result<ArchiveRollup> {
    Ok(ArchiveRollup {
        period: RollupPeriod::from_storage(row.get(0)?)?,
        bucket_start_ms: row.get(1)?,
        client: row.get(2)?,
        provider: row.get(3)?,
        model: row.get(4)?,
        cost_source: row.get(5)?,
        long_context: row.get::<_, i64>(6)? != 0,
        input: row.get(7)?,
        output: row.get(8)?,
        cache_read: row.get(9)?,
        cache_write: row.get(10)?,
        reasoning: row.get(11)?,
        messages: row.get(12)?,
        turns: row.get(13)?,
        cost_nanos: row.get(14)?,
        event_count: row.get(15)?,
        pricing_basis: toks_ingest::pricing::basis::PricingBasis {
            input: [
                row.get(16)?,
                row.get(17)?,
                row.get(18)?,
                row.get(19)?,
                row.get(20)?,
            ],
            output: [
                row.get(21)?,
                row.get(22)?,
                row.get(23)?,
                row.get(24)?,
                row.get(25)?,
            ],
            cache_read: [row.get(26)?, row.get(27)?, row.get(28)?],
            cache_write: [row.get(29)?, row.get(30)?],
        },
    })
}
