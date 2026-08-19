use anyhow::Result;
use rusqlite::Connection;
#[cfg(test)]
use tokscope_ingest::sessions::{CostSource, UnifiedMessage};
#[cfg(test)]
use tokscope_ingest::TokenBreakdown;

#[cfg(test)]
use super::{checkpoint, ArchiveCapture};

pub(super) fn initialized(connection: &Connection) -> Result<bool> {
    connection
        .query_row("SELECT EXISTS (SELECT 1 FROM events)", [], |row| row.get(0))
        .map_err(Into::into)
}

#[cfg(test)]
pub(super) fn capture(connection: &Connection) -> Result<ArchiveCapture> {
    let mut statement = connection.prepare(
        "SELECT e.event_id, e.client, e.provider, e.model, e.timestamp_ms,
         e.input_tokens, e.output_tokens, e.cache_read_tokens, e.cache_write_tokens,
         e.reasoning_tokens, e.duration_ms, e.message_count, e.is_turn_start,
         e.model_conflicted, e.cost_nanos, e.cost_source,
         COALESCE((SELECT MIN(source_hash) FROM event_sources s WHERE s.event_id=e.event_id), e.event_id)
         FROM events e ORDER BY e.timestamp_ms, e.event_id",
    )?;
    let messages = statement
        .query_map([], message_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let (captured_since_ms, captured_through_ms) = connection
        .query_row(
            "SELECT captured_since_ms, captured_through_ms FROM archive_state WHERE singleton=1",
            [],
            |row| Ok((Some(row.get(0)?), Some(row.get(1)?))),
        )
        .unwrap_or((None, None));
    let (strong_events, weak_events, conflicts) = connection.query_row(
        "SELECT
          COALESCE(SUM(confidence = 2), 0),
          COALESCE(SUM(confidence < 2), 0),
          COALESCE(SUM(conflicted = 1), 0)
         FROM events",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let pending_sources = checkpoint::pending_count(connection)?;
    Ok(ArchiveCapture {
        messages,
        captured_since_ms,
        captured_through_ms,
        strong_events,
        weak_events,
        conflicts,
        pending_sources,
    })
}

#[cfg(test)]
fn message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UnifiedMessage> {
    let event_id: String = row.get(0)?;
    let source_hash: String = row.get(16)?;
    let short_source = source_hash.get(..16).unwrap_or(&source_hash);
    let cost_nanos: i64 = row.get(14)?;
    Ok(UnifiedMessage {
        client: row.get(1)?,
        provider_id: row.get(2)?,
        model_id: row.get(3)?,
        timestamp: row.get(4)?,
        tokens: TokenBreakdown {
            input: row.get(5)?,
            output: row.get(6)?,
            cache_read: row.get(7)?,
            cache_write: row.get(8)?,
            reasoning: row.get(9)?,
        },
        duration_ms: row.get(10)?,
        message_count: i32::try_from(row.get::<_, i64>(11)?).unwrap_or(i32::MAX),
        is_turn_start: row.get::<_, i64>(12)? != 0,
        model_attribution_conflicted: row.get::<_, i64>(13)? != 0,
        cost: cost_nanos as f64 / 1_000_000_000.0,
        cost_source: cost_source(row.get(15)?),
        session_id: format!("archive:{short_source}"),
        dedup_key: Some(format!("archive:{event_id}")),
        durable_identity: None,
        accounting_aliases: Vec::new(),
        date: String::new(),
        workspace_key: None,
        workspace_label: None,
        agent: None,
        session_title: None,
    })
}

#[cfg(test)]
fn cost_source(value: i64) -> CostSource {
    match value {
        2 => CostSource::ProviderReported,
        1 => CostSource::Estimated,
        _ => CostSource::Unknown,
    }
}
