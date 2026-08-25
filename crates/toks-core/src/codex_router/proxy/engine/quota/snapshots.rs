use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::accounts::{AccountId, ProviderLimitCollection};
#[cfg(test)]
use crate::limits::LimitSnapshot;
#[cfg(test)]
use crate::rotation::account_quota_drain;
use crate::rotation::{QuotaObservation, UnixMillis};
use crate::storage::StoreUpdate;

use super::SnapshotApplication;
use crate::codex_router::proxy::engine::Engine;

type AuthFailure = (u64, Option<UnixMillis>, Option<String>);

mod candidate;
use candidate::quota_candidates;

pub(crate) struct SnapshotRefreshEpoch {
    auth_failures: BTreeMap<AccountId, AuthFailure>,
    quota_authority_revisions: BTreeMap<AccountId, u64>,
}

impl Engine {
    #[cfg(test)]
    pub fn apply_unproven_snapshots(
        &self,
        snapshots: &[LimitSnapshot],
        observed_at: DateTime<Utc>,
    ) -> Result<()> {
        let collection = ProviderLimitCollection {
            snapshots: snapshots.to_vec(),
            codex_auth: Vec::new(),
        };
        let epoch = self.begin_snapshot_refresh()?;
        self.apply_snapshots(&collection, &epoch, observed_at)
            .map(|_| ())
    }

    /// Inject a provider-authoritative observation into downstream router
    /// behavior tests without constructing credential files for every case.
    #[cfg(test)]
    pub fn apply_authoritative_snapshots_for_test(
        &self,
        snapshots: &[LimitSnapshot],
        observed_at: DateTime<Utc>,
    ) -> Result<()> {
        let discovered = self.credentials.account_ids();
        let observations = snapshots
            .iter()
            .filter(|snapshot| discovered.contains(&snapshot.account.id))
            .map(|snapshot| {
                let observation = account_quota_drain(snapshot, observed_at)
                    .map_or(QuotaObservation::ObservedAvailable, |drain| {
                        QuotaObservation::Draining(drain.reset_at)
                    });
                (snapshot.account.id.clone(), observation)
            })
            .collect();
        let at = UnixMillis::now();
        self.runtime.update(|runtime| {
            runtime.reconcile(&discovered, at);
            runtime.apply_quota_observations(&observations, at);
            runtime.heartbeat(at);
            StoreUpdate::Changed(())
        })?;
        self.reconcile_thread_overrides(at)
    }

    pub(crate) fn begin_snapshot_refresh(&self) -> Result<SnapshotRefreshEpoch> {
        let discovered = self.credentials.account_ids();
        self.runtime.latest(|runtime| {
            let auth_failures = discovered
                .iter()
                .filter_map(|account| {
                    runtime
                        .auth_failure(account)
                        .map(|failure| (account.clone(), failure))
                })
                .collect();
            let quota_authority_revisions = discovered
                .iter()
                .map(|account| (account.clone(), runtime.quota_authority_revision(account)))
                .collect();
            SnapshotRefreshEpoch {
                auth_failures,
                quota_authority_revisions,
            }
        })
    }

    pub fn apply_snapshots(
        &self,
        collection: &ProviderLimitCollection,
        epoch: &SnapshotRefreshEpoch,
        observed_at: DateTime<Utc>,
    ) -> Result<SnapshotApplication> {
        let discovered = self.credentials.account_ids();
        let candidates = quota_candidates(collection, &discovered);
        let at = UnixMillis::now();
        let mut stale_profiles = BTreeSet::new();
        let update = self.runtime.update(|runtime| {
            runtime.reconcile(&discovered, at);
            for proof in &collection.codex_auth {
                if !proof.auth_file_is_current() {
                    continue;
                }
                if let Some(failure) = epoch.auth_failures.get(proof.account_id()) {
                    runtime.sign_in_restored_by_proof(
                        proof.account_id(),
                        failure.clone(),
                        proof.credential_fingerprint(),
                    );
                }
            }
            let observations = candidates
                .iter()
                .map(|(account, candidate)| {
                    let stale = candidate.stale_profile_ids(
                        runtime
                            .accounts()
                            .get(account)
                            .and_then(|state| state.reset_acknowledged_at()),
                    );
                    if !stale.is_empty() {
                        stale_profiles.extend(stale);
                        return (account.clone(), QuotaObservation::Unknown);
                    }
                    let expected = epoch
                        .quota_authority_revisions
                        .get(account)
                        .copied()
                        .unwrap_or_default();
                    let observation = if runtime.quota_authority_revision(account) == expected {
                        candidate.observe(observed_at)
                    } else {
                        QuotaObservation::Unknown
                    };
                    (account.clone(), observation)
                })
                .collect();
            runtime.apply_quota_observations(&observations, at);
            runtime.heartbeat(at);
            StoreUpdate::Changed(())
        });
        for profile_id in &stale_profiles {
            crate::limits::live::forget_profile(crate::Provider::Codex, profile_id);
        }
        update?;
        self.reconcile_thread_overrides(at)?;
        Ok(if stale_profiles.is_empty() {
            SnapshotApplication::Applied
        } else {
            SnapshotApplication::Refetch
        })
    }
}
