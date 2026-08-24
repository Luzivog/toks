use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::limits::{self, LimitSnapshot, Provider};

use super::{
    coalesce_snapshots, discover_profiles, filter_hidden_accounts, AccountProfile, CodexAuthProof,
};

mod finalize;
use finalize::{finish_outcome, CollectedProfile};

const MAX_PARALLEL_ACCOUNTS: usize = 4;

#[derive(Clone, Copy)]
enum CollectionMode {
    Hydrate,
    Refresh,
}

/// Refresh every discovered account with bounded parallelism. Broken accounts
/// retain their last successful snapshot and an account-local typed issue.
pub fn collect_limits() -> Vec<LimitSnapshot> {
    collect(CollectionMode::Refresh, None).snapshots
}

/// Read local snapshots only. This is intentionally separate from refresh so
/// applications can paint last-known values before any network work starts.
pub fn hydrate_limits() -> Vec<LimitSnapshot> {
    collect(CollectionMode::Hydrate, None).snapshots
}

pub(crate) fn collect_provider_limits(provider: Provider) -> ProviderLimitCollection {
    collect(CollectionMode::Refresh, Some(provider))
}

pub(crate) struct ProviderLimitCollection {
    pub(crate) snapshots: Vec<LimitSnapshot>,
    pub(crate) codex_auth: Vec<CodexAuthProof>,
}

fn collect(mode: CollectionMode, provider: Option<Provider>) -> ProviderLimitCollection {
    let profiles = discover_profiles()
        .into_iter()
        .filter(|profile| provider.is_none_or(|provider| profile.provider == provider))
        .collect::<Vec<_>>();
    if profiles.is_empty() {
        return ProviderLimitCollection {
            snapshots: Vec::new(),
            codex_auth: Vec::new(),
        };
    }
    let jobs = Arc::new(Mutex::new(
        profiles.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let results = Arc::new(Mutex::new(Vec::new()));
    let workers = jobs
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .len()
        .min(MAX_PARALLEL_ACCOUNTS);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let jobs = Arc::clone(&jobs);
            let results = Arc::clone(&results);
            scope.spawn(move || loop {
                let job = jobs
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .pop_front();
                let Some((index, profile)) = job else {
                    break;
                };
                let snapshot = collect_profile(&profile, mode);
                results
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push((index, snapshot));
            });
        }
    });
    let mut results = Arc::into_inner(results)
        .expect("workers released their result handles")
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());
    results.sort_by_key(|(index, _)| *index);
    let codex_auth = results
        .iter()
        .filter_map(|(_, collected)| collected.codex_auth.clone())
        .collect();
    let snapshots = results
        .into_iter()
        .map(|(index, collected)| (index, collected.snapshot))
        .collect();
    ProviderLimitCollection {
        snapshots: filter_hidden_accounts(coalesce_snapshots(snapshots)),
        codex_auth,
    }
}

fn collect_profile(profile: &AccountProfile, mode: CollectionMode) -> CollectedProfile {
    let outcome = match mode {
        CollectionMode::Hydrate => limits::live::RefreshOutcome {
            snapshot: limits::live::hydrate(profile),
            issue: None,
            codex_auth: None,
        },
        CollectionMode::Refresh => limits::live::refresh(profile),
    };
    finish_outcome(profile, outcome)
}

#[cfg(test)]
pub(super) fn collect_profile_with(
    profile: &AccountProfile,
    refresh: impl FnOnce() -> limits::live::RefreshOutcome,
) -> LimitSnapshot {
    finish_outcome(profile, refresh()).snapshot
}

#[cfg(test)]
pub(super) fn collect_profile_with_proof(
    profile: &AccountProfile,
    refresh: impl FnOnce() -> limits::live::RefreshOutcome,
) -> (LimitSnapshot, Option<CodexAuthProof>) {
    let collected = finish_outcome(profile, refresh());
    (collected.snapshot, collected.codex_auth)
}
