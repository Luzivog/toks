use rusqlite::{Connection, OptionalExtension, Row, Transaction};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::history) struct CanonicalFact {
    pub fact_hash: String,
    pub accounting_projection_version: i64,
    pub client: String,
    pub provider: String,
    pub model: String,
    pub timestamp_ms: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub message_count: i64,
    pub is_turn_start: bool,
    pub confidence: i64,
    pub conflicted: bool,
    pub cost_source: i64,
    pub cost_nanos: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::history) struct CanonicalChange {
    pub event_id: String,
    pub before: Option<CanonicalFact>,
    pub after: Option<CanonicalFact>,
}

impl CanonicalFact {
    pub(super) fn current(
        transaction: &Transaction<'_>,
        event_id: &str,
    ) -> rusqlite::Result<Option<Self>> {
        query_current(transaction, event_id)
    }

    pub(super) fn projected(
        transaction: &Transaction<'_>,
        event_id: &str,
    ) -> rusqlite::Result<Option<Self>> {
        transaction
            .query_row(
                "SELECT fact_hash, accounting_projection_version, client, provider, model, timestamp_ms,
                 input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                 reasoning_tokens, message_count, is_turn_start, confidence,
                 conflicted, cost_source, cost_nanos
                 FROM projection_events WHERE event_id=?1",
                [event_id],
                from_row,
            )
            .optional()
    }
}

fn query_current(
    connection: &Connection,
    event_id: &str,
) -> rusqlite::Result<Option<CanonicalFact>> {
    connection
        .query_row(
            "SELECT e.canonical_fact_hash, r.accounting_projection_version,
             e.client, e.provider, e.model, e.timestamp_ms,
             e.input_tokens, e.output_tokens, e.cache_read_tokens, e.cache_write_tokens,
             e.reasoning_tokens, e.message_count, e.is_turn_start, e.confidence,
             e.conflicted, e.cost_source, e.cost_nanos
             FROM events e JOIN event_revisions r
               ON r.event_id=e.event_id AND r.fact_hash=e.canonical_fact_hash
             WHERE e.event_id=?1",
            [event_id],
            from_row,
        )
        .optional()
}

fn from_row(row: &Row<'_>) -> rusqlite::Result<CanonicalFact> {
    Ok(CanonicalFact {
        fact_hash: row.get(0)?,
        accounting_projection_version: row.get(1)?,
        client: row.get(2)?,
        provider: row.get(3)?,
        model: row.get(4)?,
        timestamp_ms: row.get(5)?,
        input: row.get(6)?,
        output: row.get(7)?,
        cache_read: row.get(8)?,
        cache_write: row.get(9)?,
        reasoning: row.get(10)?,
        message_count: row.get(11)?,
        is_turn_start: row.get::<_, i64>(12)? != 0,
        confidence: row.get(13)?,
        conflicted: row.get::<_, i64>(14)? != 0,
        cost_source: row.get(15)?,
        cost_nanos: row.get(16)?,
    })
}
