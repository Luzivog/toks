use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::batch;
use super::change::CanonicalChange;
use super::forgotten;
use super::identity;
use super::projection;
use super::store;
use super::SourceDelta;

#[cfg(test)]
pub(super) struct AppliedSource {
    pub changed: bool,
    pub changes: Vec<CanonicalChange>,
}

pub(super) fn apply(
    connection: &mut Connection,
    delta: SourceDelta<'_>,
    observed_at_ms: i64,
) -> Result<()> {
    apply_inner(connection, delta, observed_at_ms, false).map(|_| ())
}

fn apply_inner(
    connection: &mut Connection,
    delta: SourceDelta<'_>,
    observed_at_ms: i64,
    interrupt_before_checkpoint: bool,
) -> Result<(bool, Vec<CanonicalChange>)> {
    if observed_at_ms < 0 {
        bail!("usage archive capture timestamp is invalid");
    }
    let source_hash = opaque_hash("archive-source-key-v1", delta.source_key);
    let revision_hash = opaque_hash("archive-source-revision-v1", delta.revision);
    let complete = i64::from(delta.backfill_complete);
    if checkpoint_matches(connection, &source_hash, &revision_hash, complete)? {
        return Ok((false, Vec::new()));
    }

    let observations = forgotten::allowed(connection, delta.observations)?;
    let mut prepared = batch::prepare(&observations)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if checkpoint_matches(&transaction, &source_hash, &revision_hash, complete)? {
        transaction.commit()?;
        return Ok((false, Vec::new()));
    }
    let generation = store::allocate_scan_generation(&transaction)?;
    let changes = batch::apply(&transaction, &mut prepared, generation)?;
    projection::apply_changes(&transaction, &changes)?;
    if interrupt_before_checkpoint {
        bail!("simulated interruption before source checkpoint");
    }
    transaction.execute(
        "INSERT INTO source_checkpoints VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(source_hash) DO UPDATE SET
          revision_hash=excluded.revision_hash,
          captured_through_ms=MAX(captured_through_ms, excluded.captured_through_ms),
          backfill_complete=excluded.backfill_complete",
        params![source_hash, revision_hash, observed_at_ms, complete],
    )?;
    store::update_capture_state(&transaction, observed_at_ms)?;
    transaction
        .commit()
        .context("committing usage source checkpoint")?;
    Ok((true, changes))
}

pub(super) fn pending_count(connection: &Connection) -> Result<usize> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM source_checkpoints WHERE backfill_complete=0",
        [],
        |row| row.get(0),
    )?;
    usize::try_from(count).context("usage source backlog does not fit in memory")
}

fn checkpoint_matches(
    connection: &Connection,
    source_hash: &str,
    revision_hash: &str,
    complete: i64,
) -> Result<bool> {
    let known = connection
        .query_row(
            "SELECT revision_hash, backfill_complete FROM source_checkpoints
             WHERE source_hash=?1",
            [source_hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    Ok(known
        .map(|(revision, known_complete)| revision == revision_hash && known_complete == complete)
        .unwrap_or(false))
}

fn opaque_hash(domain: &str, value: &str) -> String {
    identity::fact_hash([domain.to_owned(), value.to_owned()])
}

#[cfg(test)]
pub(super) fn apply_then_interrupt(
    connection: &mut Connection,
    delta: SourceDelta<'_>,
    observed_at_ms: i64,
) -> Result<AppliedSource> {
    let (changed, changes) = apply_inner(connection, delta, observed_at_ms, true)?;
    Ok(AppliedSource { changed, changes })
}

#[cfg(test)]
pub(super) fn apply_with_report(
    connection: &mut Connection,
    delta: SourceDelta<'_>,
    observed_at_ms: i64,
) -> Result<AppliedSource> {
    let (changed, changes) = apply_inner(connection, delta, observed_at_ms, false)?;
    Ok(AppliedSource { changed, changes })
}
