//! Store-first live limit refresh with per-account backoff and in-flight deduplication.

mod memo;

use std::time::Duration;

use chrono::Utc;

use super::{LimitIssue, LimitIssueKind, LimitSnapshot, SnapshotFreshness, SnapshotStatus};
use crate::accounts::{AccountProfile, CodexAuthProof, CredentialProfileId};

const LIVE_TTL: Duration = Duration::from_secs(60);
const FAILURE_BACKOFF_MAX: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Default)]
pub(crate) struct RefreshOutcome {
    pub(crate) snapshot: Option<LimitSnapshot>,
    pub(crate) issue: Option<LimitIssue>,
    pub(crate) codex_auth: Option<CodexAuthProof>,
}

pub(super) fn failure_backoff(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(4);
    Duration::from_secs(60 * 2_u64.pow(exponent)).min(FAILURE_BACKOFF_MAX)
}

pub(crate) fn credentials_present(profile: &AccountProfile) -> bool {
    super::credentials::present(profile)
}
pub(crate) fn hydrate(profile: &AccountProfile) -> Option<LimitSnapshot> {
    super::snapshot_cache::load_or_seed(profile).map(|snapshot| normalize(snapshot, profile))
}
pub(crate) fn refresh(profile: &AccountProfile) -> RefreshOutcome {
    refresh_using(profile, || super::live_fetch::fetch(profile))
}

fn refresh_using(
    profile: &AccountProfile,
    fetch: impl FnOnce() -> Result<super::live_fetch::LiveFetch, super::http::LiveError>,
) -> RefreshOutcome {
    let baseline = hydrate(profile);
    if !credentials_present(profile) {
        return RefreshOutcome {
            snapshot: baseline,
            issue: None,
            codex_auth: None,
        };
    }

    let key = profile.cache_key();
    let credential_revision = super::credentials::revision(profile);
    let account_lock = memo::account_lock(&key);
    let _guard = account_lock
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(outcome) = memo::get(&key, baseline.clone(), credential_revision) {
        return outcome;
    }

    let previous_failures = memo::previous_failures(&key);
    let (outcome, failures, retry_for, remembered_revision) = match fetch() {
        Ok(fetched) if fetch_is_current(profile, fetched.codex_auth.as_ref()) => {
            let mut snapshot = fetched.snapshot;
            snapshot.status = SnapshotStatus::at(SnapshotFreshness::Live);
            snapshot.status.last_attempted_at = Some(Utc::now());
            snapshot.source = "live".into();
            snapshot.issue = None;
            if let Err(error) = super::snapshot_cache::store(profile, &snapshot) {
                let issue = LimitIssue::new(LimitIssueKind::Storage, error.to_string());
                snapshot.status.issue = Some(issue.clone());
            }
            if !fetch_is_current(profile, fetched.codex_auth.as_ref()) {
                let _ = super::snapshot_cache::remove_for_profile(
                    profile.provider,
                    &profile.profile_id,
                );
                return stale_codex_refresh(baseline);
            }
            (
                RefreshOutcome {
                    snapshot: Some(snapshot),
                    issue: None,
                    codex_auth: fetched.codex_auth.clone(),
                },
                0,
                LIVE_TTL,
                fetched
                    .codex_auth
                    .map(|proof| {
                        super::credentials::CredentialRevision::from_digest(proof.revision())
                    })
                    .or(credential_revision),
            )
        }
        Ok(fetched) => {
            drop(fetched);
            return stale_codex_refresh(baseline);
        }
        Err(error)
            if baseline.is_none()
                && super::settling::transient_auth_failure(profile, &error.issue) =>
        {
            return RefreshOutcome {
                snapshot: None,
                issue: Some(error.issue),
                codex_auth: None,
            };
        }
        Err(error) => {
            let (outcome, failures, retry_for) =
                failed_refresh(error.issue, baseline, previous_failures + 1);
            (outcome, failures, retry_for, credential_revision)
        }
    };
    memo::remember(
        key,
        outcome.clone(),
        failures,
        retry_for,
        remembered_revision,
    );
    outcome
}

#[cfg(test)]
pub(crate) fn refresh_for_test(
    profile: &AccountProfile,
    fetch: impl FnOnce() -> Result<super::live_fetch::LiveFetch, super::http::LiveError>,
) -> RefreshOutcome {
    refresh_using(profile, fetch)
}

fn failed_refresh(
    mut issue: LimitIssue,
    mut snapshot: Option<LimitSnapshot>,
    failures: u32,
) -> (RefreshOutcome, u32, Duration) {
    let backoff = failure_backoff(failures);
    let retry_at = issue
        .retry_at
        .filter(|retry| *retry > Utc::now())
        .unwrap_or_else(|| Utc::now() + chrono::Duration::from_std(backoff).unwrap_or_default());
    issue.retry_at = Some(retry_at);
    if let Some(last_good) = &mut snapshot {
        last_good.status = SnapshotStatus::failed(last_good.status.freshness, issue.clone());
        last_good.issue = None;
    }
    let retry_for = (retry_at - Utc::now())
        .to_std()
        .unwrap_or(backoff)
        .max(backoff);
    (
        RefreshOutcome {
            snapshot,
            issue: Some(issue),
            codex_auth: None,
        },
        failures,
        retry_for,
    )
}

fn fetch_is_current(profile: &AccountProfile, proof: Option<&CodexAuthProof>) -> bool {
    proof.is_none_or(|proof| proof.is_current(profile))
}

fn stale_codex_refresh(baseline: Option<LimitSnapshot>) -> RefreshOutcome {
    RefreshOutcome {
        snapshot: baseline,
        issue: Some(LimitIssue::new(
            LimitIssueKind::Authentication,
            "Codex credentials changed while usage was refreshing",
        )),
        codex_auth: None,
    }
}

pub(crate) fn forget_profile(provider: crate::Provider, profile_id: &CredentialProfileId) {
    memo::forget(provider, profile_id);
}

fn normalize(mut snapshot: LimitSnapshot, profile: &AccountProfile) -> LimitSnapshot {
    let email = profile
        .account
        .email
        .clone()
        .or(snapshot.account.email.take());
    snapshot.account = profile.account.clone();
    snapshot.account.email = email;
    snapshot
}
