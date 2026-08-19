use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::aliases::{self, AliasKey};
use super::candidate::Candidate;
use super::identity::Identity;

pub(super) const DIFFERENT_CONFIDENCE_SQL: &str =
    "SELECT DISTINCT r.event_id FROM event_revisions r
     JOIN events e ON e.event_id=r.event_id
     WHERE r.accounting_hash=?1 AND e.confidence != ?2
     AND EXISTS (
       SELECT 1 FROM event_sources s
       WHERE s.event_id=r.event_id AND s.source_hash=?3
     )
     LIMIT 2";

pub(super) fn exact_known(
    connection: &Connection,
    identity: &Identity,
    fact: &Candidate,
    source_hash: &str,
    alias_keys: &[AliasKey],
) -> Result<bool> {
    let known = connection
        .query_row(
            "SELECT e.event_id, e.canonical_fact_hash
             FROM identities i JOIN events e ON e.event_id=i.event_id
             WHERE i.identity_hash=?1",
            [&identity.hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((event_id, canonical_fact_hash)) = known else {
        return Ok(false);
    };
    if canonical_fact_hash != fact.fact_hash {
        return Ok(false);
    }
    let source_known: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM event_sources
         WHERE event_id=?1 AND source_hash=?2)",
        params![event_id, source_hash],
        |row| row.get(0),
    )?;
    Ok(source_known && aliases::all_attached(connection, alias_keys, &event_id)?)
}

pub(super) fn different_confidence(
    transaction: &Transaction<'_>,
    fact: &Candidate,
    source_hash: &str,
    identity: &Identity,
) -> Result<Option<String>> {
    let mut statement = transaction.prepare(DIFFERENT_CONFIDENCE_SQL)?;
    let matches = statement
        .query_map(
            params![fact.accounting_hash, identity.confidence, source_hash],
            |row| row.get(0),
        )?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok((matches.len() == 1).then(|| matches[0].clone()))
}
