use std::collections::BTreeSet;

use anyhow::Result;
use rusqlite::{params, Transaction};
use tokscope_ingest::{pricing::basis::PricingBasis, TokenBreakdown};

use super::change::{CanonicalChange, CanonicalFact};

const PERIODS: [i64; 2] = [0, 1];

pub(super) fn apply_changes(
    transaction: &Transaction<'_>,
    changes: &[CanonicalChange],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for change in changes {
        if seen.insert(change.event_id.as_str()) {
            sync_event(transaction, change)?;
        }
    }
    Ok(())
}

pub(super) fn sync_event(transaction: &Transaction<'_>, change: &CanonicalChange) -> Result<()> {
    let projected = CanonicalFact::projected(transaction, &change.event_id)?;
    if let Some(previous) = projected.as_ref() {
        apply_fact(transaction, previous, -1)?;
    }
    if let Some(current) = change.after.as_ref() {
        apply_fact(transaction, current, 1)?;
        store_fact(transaction, &change.event_id, current)?;
    } else {
        transaction.execute(
            "DELETE FROM projection_events WHERE event_id=?1",
            [&change.event_id],
        )?;
    }
    Ok(())
}

fn apply_fact(transaction: &Transaction<'_>, fact: &CanonicalFact, sign: i64) -> Result<()> {
    let output = normalized_output(fact);
    let usage = TokenBreakdown {
        input: fact.input,
        output,
        cache_read: fact.cache_read,
        cache_write: fact.cache_write,
        reasoning: fact.reasoning,
    };
    let basis = PricingBasis::from_usage(&usage);
    let long_context = i64::from(
        fact.input
            .saturating_add(fact.cache_read)
            .saturating_add(fact.cache_write)
            > 272_000,
    );
    for period in PERIODS {
        transaction.execute(
            "INSERT INTO usage_rollups VALUES (
              ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
              ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
              ?27, ?28, ?29, ?30, ?31)
             ON CONFLICT(
              period, bucket_start_ms, client, provider, model, cost_source, long_context
             )
             DO UPDATE SET
              input_tokens=input_tokens+excluded.input_tokens,
              output_tokens=output_tokens+excluded.output_tokens,
              cache_read_tokens=cache_read_tokens+excluded.cache_read_tokens,
              cache_write_tokens=cache_write_tokens+excluded.cache_write_tokens,
              reasoning_tokens=reasoning_tokens+excluded.reasoning_tokens,
              message_count=message_count+excluded.message_count,
              turn_count=turn_count+excluded.turn_count,
              cost_nanos=cost_nanos+excluded.cost_nanos,
              event_count=event_count+excluded.event_count,
              input_b0=input_b0+excluded.input_b0,
              input_b1=input_b1+excluded.input_b1,
              input_b2=input_b2+excluded.input_b2,
              input_b3=input_b3+excluded.input_b3,
              input_b4=input_b4+excluded.input_b4,
              output_b0=output_b0+excluded.output_b0,
              output_b1=output_b1+excluded.output_b1,
              output_b2=output_b2+excluded.output_b2,
              output_b3=output_b3+excluded.output_b3,
              output_b4=output_b4+excluded.output_b4,
              cache_read_b0=cache_read_b0+excluded.cache_read_b0,
              cache_read_b1=cache_read_b1+excluded.cache_read_b1,
              cache_read_b2=cache_read_b2+excluded.cache_read_b2,
              cache_write_b0=cache_write_b0+excluded.cache_write_b0,
              cache_write_b1=cache_write_b1+excluded.cache_write_b1",
            params![
                period,
                bucket_start(period, fact.timestamp_ms),
                fact.client,
                fact.provider,
                fact.model,
                fact.cost_source,
                long_context,
                sign * fact.input,
                sign * output,
                sign * fact.cache_read,
                sign * fact.cache_write,
                sign * fact.reasoning,
                sign * fact.message_count,
                sign * i64::from(fact.is_turn_start),
                sign * fact.cost_nanos,
                sign,
                sign * basis.input[0],
                sign * basis.input[1],
                sign * basis.input[2],
                sign * basis.input[3],
                sign * basis.input[4],
                sign * basis.output[0],
                sign * basis.output[1],
                sign * basis.output[2],
                sign * basis.output[3],
                sign * basis.output[4],
                sign * basis.cache_read[0],
                sign * basis.cache_read[1],
                sign * basis.cache_read[2],
                sign * basis.cache_write[0],
                sign * basis.cache_write[1],
            ],
        )?;
    }
    transaction.execute("DELETE FROM usage_rollups WHERE event_count=0", [])?;
    transaction.execute(
        "UPDATE projection_state SET
          strong_events=strong_events + ?1,
          weak_events=weak_events + ?2,
          conflicts=conflicts + ?3
         WHERE singleton=1",
        params![
            sign * i64::from(fact.confidence == 2),
            sign * i64::from(fact.confidence < 2),
            sign * i64::from(fact.conflicted),
        ],
    )?;
    Ok(())
}

fn store_fact(transaction: &Transaction<'_>, event_id: &str, fact: &CanonicalFact) -> Result<()> {
    transaction.execute(
        "INSERT INTO projection_events VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
          ?15, ?16, ?17, ?18)
         ON CONFLICT(event_id) DO UPDATE SET
          fact_hash=excluded.fact_hash,
          accounting_projection_version=excluded.accounting_projection_version,
          client=excluded.client, provider=excluded.provider,
          model=excluded.model, timestamp_ms=excluded.timestamp_ms,
          input_tokens=excluded.input_tokens, output_tokens=excluded.output_tokens,
          cache_read_tokens=excluded.cache_read_tokens,
          cache_write_tokens=excluded.cache_write_tokens,
          reasoning_tokens=excluded.reasoning_tokens,
          message_count=excluded.message_count, is_turn_start=excluded.is_turn_start,
          confidence=excluded.confidence, conflicted=excluded.conflicted,
          cost_source=excluded.cost_source, cost_nanos=excluded.cost_nanos",
        params![
            event_id,
            fact.fact_hash,
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
            fact.message_count,
            i64::from(fact.is_turn_start),
            fact.confidence,
            i64::from(fact.conflicted),
            fact.cost_source,
            fact.cost_nanos,
        ],
    )?;
    Ok(())
}

fn normalized_output(fact: &CanonicalFact) -> i64 {
    if fact.client == "codex" && fact.accounting_projection_version < 2 {
        fact.output.saturating_sub(fact.reasoning).max(0)
    } else {
        fact.output
    }
}

fn bucket_start(period: i64, timestamp_ms: i64) -> i64 {
    match period {
        0 => 0,
        1 => timestamp_ms.div_euclid(60_000) * 60_000,
        _ => unreachable!("projection periods are fixed"),
    }
}
