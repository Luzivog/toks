use anyhow::Result;
use rusqlite::Transaction;
use tokscope_ingest::sessions::UnifiedMessage;

use super::aliases::{self, AliasKey};
use super::candidate::Candidate;
use super::change::CanonicalChange;
use super::identity::{self, Identity};
use super::{lookup, store};

pub(super) struct PreparedObservation {
    candidate: Candidate,
    identity: Identity,
    event_source_hash: String,
    alias_keys: Vec<AliasKey>,
}

pub(super) fn prepare(observations: &[UnifiedMessage]) -> Result<Vec<PreparedObservation>> {
    observations
        .iter()
        .map(|message| {
            let candidate = Candidate::from_message(message, 1)?;
            Ok(PreparedObservation {
                identity: Identity::for_observation(message, &candidate),
                candidate,
                event_source_hash: identity::source_hash(message),
                alias_keys: aliases::keys(message),
            })
        })
        .collect()
}

pub(super) fn apply(
    transaction: &Transaction<'_>,
    prepared: &mut [PreparedObservation],
    generation: i64,
) -> Result<Vec<CanonicalChange>> {
    let mut changes = Vec::new();
    for item in prepared {
        if exact_known(transaction, item)? {
            continue;
        }
        item.candidate.first_observed_generation = generation;
        if let Some(change) = store::accept(
            transaction,
            item.candidate.clone(),
            Identity {
                hash: item.identity.hash.clone(),
                scheme: item.identity.scheme,
                version: item.identity.version,
                confidence: item.identity.confidence,
            },
            &item.event_source_hash,
            &item.alias_keys,
            generation,
        )? {
            changes.push(change);
        }
    }
    Ok(changes)
}

#[cfg(test)]
pub(super) fn revision(observations: &[UnifiedMessage]) -> Result<String> {
    let prepared = prepare(observations)?;
    let mut parts = prepared
        .iter()
        .map(|item| format!("{}:{}", item.identity.hash, item.candidate.fact_hash))
        .collect::<Vec<_>>();
    parts.sort_unstable();
    parts.insert(0, "archive-source-revision-v1".into());
    Ok(identity::fact_hash(parts))
}

fn exact_known(transaction: &Transaction<'_>, item: &PreparedObservation) -> Result<bool> {
    lookup::exact_known(
        transaction,
        &item.identity,
        &item.candidate,
        &item.event_source_hash,
        &item.alias_keys,
    )
}
