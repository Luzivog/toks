use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::accounts::{AccountId, CodexAuthProof, CredentialProfileId, ProviderLimitCollection};
use crate::limits::{LimitSnapshot, Provider, SnapshotFreshness};
use crate::rotation::{account_quota_drain, QuotaObservation, UnixMillis};

pub(super) enum QuotaCandidate<'a> {
    Proved {
        snapshot: &'a LimitSnapshot,
        proof: &'a CodexAuthProof,
    },
    Unknown,
}

impl QuotaCandidate<'_> {
    pub(super) fn stale_profile_ids(
        &self,
        reset_acknowledged_at: Option<UnixMillis>,
    ) -> Vec<CredentialProfileId> {
        let (Self::Proved { snapshot, proof }, Some(acknowledged_at)) =
            (self, reset_acknowledged_at)
        else {
            return Vec::new();
        };
        if snapshot
            .fetched_at
            .is_some_and(|fetched_at| fetched_at.timestamp_millis() > acknowledged_at.get())
        {
            return Vec::new();
        }
        let mut profiles = snapshot
            .account
            .sources
            .iter()
            .map(|source| source.profile_id.clone())
            .collect::<Vec<_>>();
        if profiles.is_empty() {
            profiles.push(proof.profile_id().clone());
        }
        profiles.sort();
        profiles.dedup();
        profiles
    }

    pub(super) fn observe(&self, observed_at: DateTime<Utc>) -> QuotaObservation {
        let Self::Proved { snapshot, proof } = self else {
            return QuotaObservation::Unknown;
        };
        if !proof.auth_file_is_current() {
            return QuotaObservation::Unknown;
        }
        let mut found = false;
        for window in snapshot
            .windows
            .iter()
            .filter(|window| window.scope.is_none() && !window.reset_elapsed(observed_at))
        {
            if !window.percent_used.is_finite() || !(0.0..=100.0).contains(&window.percent_used) {
                return QuotaObservation::Unknown;
            }
            found = true;
        }
        if !found {
            return QuotaObservation::Unknown;
        }
        account_quota_drain(snapshot, observed_at)
            .map_or(QuotaObservation::ObservedAvailable, |drain| {
                QuotaObservation::Draining(drain.reset_at)
            })
    }
}

pub(super) fn quota_candidates<'a>(
    collection: &'a ProviderLimitCollection,
    discovered: &[AccountId],
) -> BTreeMap<AccountId, QuotaCandidate<'a>> {
    discovered
        .iter()
        .map(|account| {
            let snapshot = exactly_one(
                collection
                    .snapshots
                    .iter()
                    .filter(|snapshot| &snapshot.account.id == account),
            )
            .filter(|snapshot| authoritative(snapshot));
            let proof = snapshot
                .and_then(|snapshot| snapshot.account.primary_source())
                .and_then(|source| {
                    exactly_one(collection.codex_auth.iter().filter(|proof| {
                        proof.account_id() == account && proof.profile_id() == &source.profile_id
                    }))
                });
            let candidate = match (snapshot, proof) {
                (Some(snapshot), Some(proof)) => QuotaCandidate::Proved { snapshot, proof },
                _ => QuotaCandidate::Unknown,
            };
            (account.clone(), candidate)
        })
        .collect()
}

fn authoritative(snapshot: &LimitSnapshot) -> bool {
    snapshot.provider == Provider::Codex
        && snapshot.status.freshness == SnapshotFreshness::Live
        && snapshot.status.issue.is_none()
        && snapshot.issue.is_none()
        && !snapshot.windows.is_empty()
}

fn exactly_one<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let value = values.next()?;
    values.next().is_none().then_some(value)
}
