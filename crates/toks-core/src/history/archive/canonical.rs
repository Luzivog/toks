use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use rusqlite::{params, Transaction};

use super::candidate::Candidate;

const MAX_REVISIONS_PER_EVENT: usize = 4;
const CLAUDE_RESPONSE_SCHEME: &str = "claude-provider-response";

pub(super) fn select(transaction: &Transaction<'_>, event_id: &str) -> Result<()> {
    let scheme: String = transaction.query_row(
        "SELECT identity_scheme FROM events WHERE event_id=?1",
        [event_id],
        |row| row.get(0),
    )?;
    let mut candidates = load_candidates(transaction, event_id)?;
    let (best_hash, conflict) = if scheme == CLAUDE_RESPONSE_SCHEME {
        select_claude_completion(&candidates)
    } else {
        (
            select_first_observation(&candidates)?.fact_hash.clone(),
            false,
        )
    };
    let best = candidates
        .iter()
        .find(|candidate| candidate.fact_hash == best_hash)
        .context("selected usage revision is missing")?
        .clone();
    update(transaction, event_id, &best)?;
    if conflict {
        transaction.execute(
            "UPDATE events SET conflicted=1 WHERE event_id=?1",
            [event_id],
        )?;
    }
    candidates.sort_by(|left, right| {
        (left.fact_hash != best_hash)
            .cmp(&(right.fact_hash != best_hash))
            .then_with(|| {
                left.first_observed_generation
                    .cmp(&right.first_observed_generation)
            })
            .then_with(|| left.fact_hash.cmp(&right.fact_hash))
    });
    for stale in candidates.iter().skip(MAX_REVISIONS_PER_EVENT) {
        transaction.execute(
            "DELETE FROM event_revisions WHERE event_id=?1 AND fact_hash=?2",
            params![event_id, stale.fact_hash],
        )?;
    }
    Ok(())
}

fn load_candidates(transaction: &Transaction<'_>, event_id: &str) -> Result<Vec<Candidate>> {
    let mut statement = transaction.prepare(
        "SELECT fact_hash, accounting_hash, accounting_projection_version, client, provider,
         model, timestamp_ms, input_tokens, output_tokens, cache_read_tokens,
         cache_write_tokens, reasoning_tokens, duration_ms, message_count, is_turn_start,
         model_conflicted, cost_nanos, cost_source, first_observed_generation
         FROM event_revisions WHERE event_id=?1",
    )?;
    let candidates = statement
        .query_map([event_id], Candidate::from_revision)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(candidates)
}

fn select_first_observation(candidates: &[Candidate]) -> Result<&Candidate> {
    let mut first_by_accounting = HashMap::<&str, i64>::new();
    for candidate in candidates {
        first_by_accounting
            .entry(&candidate.accounting_hash)
            .and_modify(|first| *first = (*first).min(candidate.first_observed_generation))
            .or_insert(candidate.first_observed_generation);
    }
    candidates
        .iter()
        .min_by(|left, right| {
            first_by_accounting[left.accounting_hash.as_str()]
                .cmp(&first_by_accounting[right.accounting_hash.as_str()])
                .then_with(|| left.accounting_hash.cmp(&right.accounting_hash))
                .then_with(|| right.cost_source.cmp(&left.cost_source))
                .then_with(|| {
                    left.first_observed_generation
                        .cmp(&right.first_observed_generation)
                })
                .then_with(|| left.fact_hash.cmp(&right.fact_hash))
        })
        .context("usage event has no revision")
}

fn select_claude_completion(candidates: &[Candidate]) -> (String, bool) {
    let mut groups = BTreeMap::<i64, Vec<&Candidate>>::new();
    for candidate in candidates {
        groups
            .entry(candidate.first_observed_generation)
            .or_default()
            .push(candidate);
    }
    let mut selected: Option<&Candidate> = None;
    let mut conflict = false;
    for group in groups.values() {
        let eligible = selected
            .map(|current| {
                group
                    .iter()
                    .all(|candidate| candidate.is_monotonic_extension_of(current))
            })
            .unwrap_or(true);
        if !eligible {
            conflict = true;
            continue;
        }
        let maxima = group.iter().copied().filter(|candidate| {
            group
                .iter()
                .all(|other| candidate.is_monotonic_extension_of(other))
        });
        let next = maxima.max_by(|left, right| {
            left.cost_source
                .cmp(&right.cost_source)
                .then_with(|| right.fact_hash.cmp(&left.fact_hash))
        });
        if let Some(next) = next {
            selected = Some(next);
        } else {
            conflict = true;
            if selected.is_none() {
                selected = group.iter().copied().min_by_key(|item| &item.fact_hash);
            }
        }
    }
    let selected = selected.expect("event revisions are nonempty");
    (selected.fact_hash.clone(), conflict)
}

fn update(transaction: &Transaction<'_>, event_id: &str, fact: &Candidate) -> Result<()> {
    transaction.execute(
        "UPDATE events SET canonical_fact_hash=?2, canonical_accounting_hash=?3,
         client=?4, provider=?5, model=?6, timestamp_ms=?7, input_tokens=?8,
         output_tokens=?9, cache_read_tokens=?10, cache_write_tokens=?11,
         reasoning_tokens=?12, duration_ms=?13, message_count=?14, is_turn_start=?15,
         model_conflicted=?16, cost_nanos=?17, cost_source=?18 WHERE event_id=?1",
        params![
            event_id,
            fact.fact_hash,
            fact.accounting_hash,
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
        ],
    )?;
    Ok(())
}
