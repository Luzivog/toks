use anyhow::Result;
use rusqlite::{params, OptionalExtension, Transaction};

use super::aliases::{self, AliasKey, Lookup};
use super::candidate::Candidate;
use super::identity::{self, Identity};
use super::lookup;

pub(super) struct EventResolution {
    pub event_id: String,
    pub created: bool,
    pub equivalent_alias: bool,
    pub unchanged_fact: bool,
}

pub(super) fn event(
    transaction: &Transaction<'_>,
    identity: &Identity,
    fact: &Candidate,
    source_hash: &str,
    alias_keys: &[AliasKey],
) -> Result<EventResolution> {
    let known = transaction
        .query_row(
            "SELECT i.event_id, e.canonical_fact_hash FROM identities i
             JOIN events e ON e.event_id=i.event_id WHERE i.identity_hash=?1",
            [&identity.hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let alias_match = aliases::lookup(transaction, alias_keys, fact)?;
    if let Some((event_id, canonical_fact_hash)) = known {
        return resolve_known(
            transaction,
            alias_keys,
            event_id,
            canonical_fact_hash == fact.fact_hash,
            alias_match,
        );
    }

    match alias_match {
        Lookup::Equivalent(event_id) => {
            insert_identity(transaction, identity, &event_id)?;
            aliases::attach(transaction, alias_keys, &event_id)?;
            return Ok(EventResolution {
                event_id,
                created: false,
                equivalent_alias: true,
                unchanged_fact: true,
            });
        }
        Lookup::Collision(related) => {
            let event_id = create_event(transaction, identity, fact)?;
            aliases::quarantine(transaction, alias_keys, &event_id, &related)?;
            return Ok(EventResolution {
                event_id,
                created: true,
                equivalent_alias: false,
                unchanged_fact: false,
            });
        }
        Lookup::None => {}
    }
    if let Some(event_id) = lookup::different_confidence(transaction, fact, source_hash, identity)?
    {
        insert_identity(transaction, identity, &event_id)?;
        transaction.execute(
            "UPDATE events SET confidence=?2, identity_scheme=?3, identity_version=?4
             WHERE event_id=?1 AND confidence < ?2",
            params![
                event_id,
                identity.confidence,
                identity.scheme,
                identity.version
            ],
        )?;
        aliases::attach(transaction, alias_keys, &event_id)?;
        return Ok(EventResolution {
            event_id,
            created: false,
            equivalent_alias: false,
            unchanged_fact: false,
        });
    }

    let event_id = create_event(transaction, identity, fact)?;
    aliases::attach(transaction, alias_keys, &event_id)?;
    Ok(EventResolution {
        event_id,
        created: true,
        equivalent_alias: false,
        unchanged_fact: false,
    })
}

fn resolve_known(
    transaction: &Transaction<'_>,
    alias_keys: &[AliasKey],
    event_id: String,
    unchanged_fact: bool,
    alias_match: Lookup,
) -> Result<EventResolution> {
    let equivalent_alias = match alias_match {
        Lookup::None => {
            aliases::attach(transaction, alias_keys, &event_id)?;
            false
        }
        Lookup::Equivalent(alias_event) if alias_event == event_id => {
            aliases::attach(transaction, alias_keys, &event_id)?;
            true
        }
        Lookup::Equivalent(alias_event) => {
            aliases::quarantine(transaction, alias_keys, &event_id, &[alias_event])?;
            false
        }
        Lookup::Collision(related) => {
            aliases::quarantine(transaction, alias_keys, &event_id, &related)?;
            false
        }
    };
    Ok(EventResolution {
        event_id,
        created: false,
        equivalent_alias,
        unchanged_fact,
    })
}

fn create_event(
    transaction: &Transaction<'_>,
    identity: &Identity,
    fact: &Candidate,
) -> Result<String> {
    let event_id = identity::event_id(&identity.hash);
    insert_event(transaction, &event_id, identity, fact)?;
    insert_identity(transaction, identity, &event_id)?;
    Ok(event_id)
}

fn insert_identity(
    transaction: &Transaction<'_>,
    identity: &Identity,
    event_id: &str,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO identities
         (identity_hash, scheme, version, confidence, event_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            identity.hash,
            identity.scheme,
            identity.version,
            identity.confidence,
            event_id,
        ],
    )?;
    Ok(())
}

fn insert_event(
    transaction: &Transaction<'_>,
    event_id: &str,
    identity: &Identity,
    fact: &Candidate,
) -> Result<()> {
    transaction.execute(
        "INSERT OR IGNORE INTO events VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
          ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
        params![
            event_id,
            identity.scheme,
            identity.version,
            identity.confidence,
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
