use anyhow::Result;
use rusqlite::{params, Transaction};

use super::aliases::AliasKey;
use super::candidate::Candidate;
use super::canonical;
use super::change::{CanonicalChange, CanonicalFact};
use super::identity::Identity;
use super::resolve;

pub(super) fn accept(
    transaction: &Transaction<'_>,
    candidate: Candidate,
    identity: Identity,
    source_hash: &str,
    alias_keys: &[AliasKey],
    scan_generation: i64,
) -> Result<Option<CanonicalChange>> {
    let resolution = resolve::event(transaction, &identity, &candidate, source_hash, alias_keys)?;
    let event_id = resolution.event_id;
    insert_source(transaction, &event_id, source_hash, scan_generation)?;
    if resolution.equivalent_alias || resolution.unchanged_fact {
        return Ok(None);
    }
    let before = if resolution.created {
        None
    } else {
        CanonicalFact::projected(transaction, &event_id)?
            .or(CanonicalFact::current(transaction, &event_id)?)
    };
    let (canonical_accounting, canonical_fact, canonical_cost_source): (String, String, i64) =
        transaction.query_row(
            "SELECT canonical_accounting_hash, canonical_fact_hash, cost_source
             FROM events WHERE event_id = ?1",
            [&event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    let reported_cost_conflict = canonical_accounting == candidate.accounting_hash
        && canonical_fact != candidate.fact_hash
        && canonical_cost_source == 2
        && candidate.cost_source == 2;
    let accounting_conflict = identity.scheme != "claude-provider-response"
        && canonical_accounting != candidate.accounting_hash;
    if accounting_conflict || reported_cost_conflict {
        transaction.execute(
            "UPDATE events SET conflicted = 1 WHERE event_id = ?1",
            [&event_id],
        )?;
    }

    upsert_revision(transaction, &event_id, &candidate)?;
    canonical::select(transaction, &event_id)?;
    let after = CanonicalFact::current(transaction, &event_id)?;
    if before == after {
        return Ok(None);
    }
    Ok(Some(CanonicalChange {
        event_id,
        before,
        after,
    }))
}

fn upsert_revision(transaction: &Transaction<'_>, event_id: &str, fact: &Candidate) -> Result<()> {
    transaction.execute(
        "INSERT INTO event_revisions VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
          ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT(event_id, fact_hash) DO UPDATE SET
          first_observed_generation = MIN(
            first_observed_generation, excluded.first_observed_generation
          )",
        params![
            event_id,
            fact.fact_hash,
            fact.accounting_hash,
            fact.accounting_projection_version,
            fact.client,
            fact.provider,
            fact.model,
            fact.timestamp_ms,
            fact.input,
            fact.output,
            fact.cache_read,
            fact.cache_write,
            fact.reasoning,
            fact.duration_ms,
            fact.message_count,
            i64::from(fact.is_turn_start),
            i64::from(fact.model_conflicted),
            fact.cost_nanos,
            fact.cost_source,
            fact.first_observed_generation,
        ],
    )?;
    Ok(())
}

pub(super) fn update_capture_state(
    transaction: &Transaction<'_>,
    observed_at_ms: i64,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO archive_state (singleton, captured_since_ms, captured_through_ms)
         SELECT 1, ?1, ?1 WHERE EXISTS (SELECT 1 FROM events)
         ON CONFLICT(singleton) DO UPDATE SET
          captured_since_ms = MIN(captured_since_ms, excluded.captured_since_ms),
          captured_through_ms = MAX(captured_through_ms, excluded.captured_through_ms)
         WHERE excluded.captured_since_ms < captured_since_ms
            OR excluded.captured_through_ms > captured_through_ms",
        [observed_at_ms],
    )?;
    Ok(())
}

fn insert_source(
    transaction: &Transaction<'_>,
    event_id: &str,
    source_hash: &str,
    scan_generation: i64,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO event_sources VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(event_id, source_hash) DO NOTHING",
        params![event_id, source_hash, scan_generation],
    )?;
    Ok(())
}

pub(super) fn allocate_scan_generation(transaction: &Transaction<'_>) -> Result<i64> {
    transaction.execute(
        "INSERT INTO archive_clock (singleton, last_scan_generation) VALUES (1, 1)
         ON CONFLICT(singleton) DO UPDATE SET
          last_scan_generation = last_scan_generation + 1",
        [],
    )?;
    Ok(transaction.query_row(
        "SELECT last_scan_generation FROM archive_clock WHERE singleton=1",
        [],
        |row| row.get(0),
    )?)
}
