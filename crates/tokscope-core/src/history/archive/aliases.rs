use std::collections::BTreeSet;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use tokscope_ingest::{AccountingAliasScheme, UnifiedMessage};

use super::candidate::Candidate;
use super::identity;

pub(super) struct AliasKey {
    hash: String,
    scheme: &'static str,
    version: i64,
}

pub(super) enum Lookup {
    None,
    Equivalent(String),
    Collision(Vec<String>),
}

pub(super) fn keys(message: &UnifiedMessage) -> Vec<AliasKey> {
    message
        .accounting_aliases
        .iter()
        .map(|alias| {
            let scheme = match alias.scheme {
                AccountingAliasScheme::CodexForkReplayDedup => "codex-fork-replay-dedup",
            };
            AliasKey {
                hash: identity::accounting_alias_hash(scheme, alias.version, &alias.value),
                scheme,
                version: i64::from(alias.version),
            }
        })
        .collect()
}

pub(super) fn all_attached(
    connection: &Connection,
    aliases: &[AliasKey],
    event_id: &str,
) -> Result<bool> {
    for alias in aliases {
        let attached = connection
            .query_row(
                "SELECT event_id FROM accounting_aliases WHERE alias_hash=?1",
                [&alias.hash],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if attached.as_deref() != Some(event_id) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn lookup(
    transaction: &Transaction<'_>,
    aliases: &[AliasKey],
    fact: &Candidate,
) -> Result<Lookup> {
    let mut event_ids = BTreeSet::new();
    let mut conflicted = false;
    for alias in aliases {
        let found = transaction
            .query_row(
                "SELECT event_id, conflicted FROM accounting_aliases WHERE alias_hash=?1",
                [&alias.hash],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?;
        if let Some((event_id, is_conflicted)) = found {
            event_ids.insert(event_id);
            conflicted |= is_conflicted;
        }
    }
    if event_ids.is_empty() {
        return Ok(Lookup::None);
    }
    let ids = event_ids.into_iter().collect::<Vec<_>>();
    if conflicted || ids.len() != 1 {
        return Ok(Lookup::Collision(ids));
    }
    let canonical = canonical_fact(transaction, &ids[0])?;
    if equivalent_accounting(fact, &canonical) {
        Ok(Lookup::Equivalent(ids[0].clone()))
    } else {
        Ok(Lookup::Collision(ids))
    }
}

/// Aliases may link scan representatives with different local timestamps, but
/// every value that contributes to usage must agree.
fn equivalent_accounting(left: &Candidate, right: &Candidate) -> bool {
    left.accounting_projection_version == right.accounting_projection_version
        && left.client == right.client
        && left.provider == right.provider
        && left.model == right.model
        && left.input == right.input
        && left.output == right.output
        && left.cache_read == right.cache_read
        && left.cache_write == right.cache_write
        && left.reasoning == right.reasoning
        && left.message_count == right.message_count
        && left.is_turn_start == right.is_turn_start
        && left.model_conflicted == right.model_conflicted
        && left.cost_source == right.cost_source
        && left.cost_nanos == right.cost_nanos
}

pub(super) fn attach(
    transaction: &Transaction<'_>,
    aliases: &[AliasKey],
    event_id: &str,
) -> Result<()> {
    for alias in aliases {
        transaction.execute(
            "INSERT INTO accounting_aliases
             (alias_hash, scheme, version, event_id, conflicted)
             VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT(alias_hash) DO NOTHING",
            params![alias.hash, alias.scheme, alias.version, event_id],
        )?;
    }
    Ok(())
}

pub(super) fn quarantine(
    transaction: &Transaction<'_>,
    aliases: &[AliasKey],
    primary_event: &str,
    related_events: &[String],
) -> Result<()> {
    transaction.execute(
        "UPDATE events SET conflicted=1 WHERE event_id=?1",
        [primary_event],
    )?;
    for event_id in related_events {
        transaction.execute(
            "UPDATE events SET conflicted=1 WHERE event_id=?1",
            [event_id],
        )?;
    }
    for alias in aliases {
        transaction.execute(
            "INSERT INTO accounting_aliases
             (alias_hash, scheme, version, event_id, conflicted)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(alias_hash) DO UPDATE SET conflicted=1",
            params![alias.hash, alias.scheme, alias.version, primary_event],
        )?;
    }
    Ok(())
}

fn canonical_fact(transaction: &Transaction<'_>, event_id: &str) -> Result<Candidate> {
    transaction
        .query_row(
            "SELECT r.fact_hash, r.accounting_hash, r.accounting_projection_version,
             r.client, r.provider, r.model, r.timestamp_ms, r.input_tokens,
             r.output_tokens, r.cache_read_tokens, r.cache_write_tokens,
             r.reasoning_tokens, r.duration_ms, r.message_count, r.is_turn_start,
             r.model_conflicted, r.cost_nanos, r.cost_source, r.first_observed_generation
             FROM events e JOIN event_revisions r
              ON r.event_id=e.event_id AND r.fact_hash=e.canonical_fact_hash
             WHERE e.event_id=?1",
            [event_id],
            Candidate::from_revision,
        )
        .context("accounting alias points to an event without canonical facts")
}
