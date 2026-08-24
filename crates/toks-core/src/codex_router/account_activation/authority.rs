use crate::accounts::{AccountId, AccountProfile, CredentialProfileId, ProviderLimitCollection};
use crate::limits::{LimitSnapshot, Provider, SnapshotFreshness};

#[derive(Clone, Debug)]
pub(super) struct Authority {
    pub(super) account: AccountId,
    pub(super) profile_id: CredentialProfileId,
    pub(super) fetched_at_ms: i64,
    pub(super) weekly: Option<WeeklyUsage>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WeeklyUsage {
    pub(super) percent_used: f64,
    pub(super) resets_at_ms: i64,
}

pub(super) fn proved(collection: &ProviderLimitCollection, now_ms: i64) -> Vec<Authority> {
    let profiles = crate::accounts::discover_profiles();
    proved_with(collection, &profiles, now_ms)
}

fn proved_with(
    collection: &ProviderLimitCollection,
    profiles: &[AccountProfile],
    now_ms: i64,
) -> Vec<Authority> {
    collection
        .snapshots
        .iter()
        .filter(|snapshot| authoritative(snapshot))
        .filter(|snapshot| {
            collection
                .snapshots
                .iter()
                .filter(|candidate| candidate.account.id == snapshot.account.id)
                .count()
                == 1
        })
        .filter_map(|snapshot| {
            let source = snapshot.account.primary_source()?;
            let proof = exactly_one(collection.codex_auth.iter().filter(|proof| {
                proof.account_id() == &snapshot.account.id
                    && proof.profile_id() == &source.profile_id
            }))?;
            let profile = exactly_one(profiles.iter().filter(|profile| proof.is_current(profile)))?;
            let fetched_at_ms = snapshot.fetched_at?.timestamp_millis();
            Some(Authority {
                account: snapshot.account.id.clone(),
                profile_id: profile.profile_id.clone(),
                fetched_at_ms,
                weekly: weekly(snapshot, now_ms),
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn proved_for_test(
    collection: &ProviderLimitCollection,
    profiles: &[AccountProfile],
    now_ms: i64,
) -> Vec<Authority> {
    proved_with(collection, profiles, now_ms)
}

fn authoritative(snapshot: &LimitSnapshot) -> bool {
    snapshot.provider == Provider::Codex
        && snapshot.status.freshness == SnapshotFreshness::Live
        && snapshot.status.issue.is_none()
        && snapshot.issue.is_none()
}

fn weekly(snapshot: &LimitSnapshot, now_ms: i64) -> Option<WeeklyUsage> {
    let window = exactly_one(
        snapshot
            .windows
            .iter()
            .filter(|window| window.scope.is_none() && window.label == "Weekly"),
    )?;
    if !window.percent_used.is_finite() || !(0.0..=100.0).contains(&window.percent_used) {
        return None;
    }
    let resets_at_ms = window.resets_at?.timestamp_millis();
    (resets_at_ms > now_ms).then_some(WeeklyUsage {
        percent_used: window.percent_used,
        resets_at_ms,
    })
}

fn exactly_one<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let value = values.next()?;
    values.next().is_none().then_some(value)
}
