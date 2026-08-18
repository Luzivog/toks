//! Store-first live limit refresh with per-account backoff and in-flight deduplication.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::Utc;

use super::{LimitIssue, LimitIssueKind, LimitSnapshot, SnapshotFreshness, SnapshotStatus};
use crate::accounts::AccountProfile;

const LIVE_TTL: Duration = Duration::from_secs(60);
const FAILURE_BACKOFF_MAX: Duration = Duration::from_secs(15 * 60);
#[derive(Clone)]
struct MemoEntry {
    attempted_at: Instant,
    outcome: RefreshOutcome,
    failures: u32,
    retry_for: Duration,
    credential_revision: Option<super::credentials::CredentialRevision>,
}

#[derive(Clone, Default)]
pub(crate) struct RefreshOutcome {
    pub(crate) snapshot: Option<LimitSnapshot>,
    pub(crate) issue: Option<LimitIssue>,
}

type AccountLocks = HashMap<String, Arc<Mutex<()>>>;
static MEMO: OnceLock<Mutex<HashMap<String, MemoEntry>>> = OnceLock::new();
static LOCKS: OnceLock<Mutex<AccountLocks>> = OnceLock::new();
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
    let baseline = hydrate(profile);
    if !credentials_present(profile) {
        return RefreshOutcome {
            snapshot: baseline,
            issue: None,
        };
    }

    let key = profile.cache_key();
    let credential_revision = super::credentials::revision(profile);
    let account_lock = account_lock(&key);
    let _guard = account_lock
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(outcome) = memoized(&key, baseline.clone(), credential_revision) {
        return outcome;
    }

    let previous_failures = memo()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .map(|entry| entry.failures)
        .unwrap_or(0);
    let (outcome, failures, retry_for) = match super::live_fetch::fetch(profile) {
        Ok(mut snapshot) => {
            snapshot.status = SnapshotStatus::at(SnapshotFreshness::Live);
            snapshot.status.last_attempted_at = Some(Utc::now());
            snapshot.source = "live".into();
            snapshot.issue = None;
            if let Err(error) = super::snapshot_cache::store(profile, &snapshot) {
                let issue = LimitIssue::new(LimitIssueKind::Storage, error.to_string());
                snapshot.status.issue = Some(issue.clone());
            }
            (
                RefreshOutcome {
                    snapshot: Some(snapshot),
                    issue: None,
                },
                0,
                LIVE_TTL,
            )
        }
        Err(error)
            if baseline.is_none()
                && super::settling::transient_auth_failure(profile, &error.issue) =>
        {
            return RefreshOutcome {
                snapshot: None,
                issue: Some(error.issue),
            };
        }
        Err(error) => failed_refresh(error.issue, baseline, previous_failures + 1),
    };
    memo()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(
            key,
            MemoEntry {
                attempted_at: Instant::now(),
                outcome: outcome.clone(),
                failures,
                retry_for,
                credential_revision: super::credentials::revision(profile),
            },
        );
    outcome
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
        },
        failures,
        retry_for,
    )
}

fn memoized(
    key: &str,
    baseline: Option<LimitSnapshot>,
    credential_revision: Option<super::credentials::CredentialRevision>,
) -> Option<RefreshOutcome> {
    let entries = memo().lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = entries.get(key)?;
    if entry.credential_revision != credential_revision
        || entry.attempted_at.elapsed() >= entry.retry_for
    {
        return None;
    }
    let mut outcome = entry.outcome.clone();
    if is_newer(&baseline, &outcome.snapshot) {
        outcome.snapshot = baseline.map(|mut snapshot| {
            if let Some(issue) = &outcome.issue {
                snapshot.status = SnapshotStatus::failed(snapshot.status.freshness, issue.clone());
            }
            snapshot
        });
    }
    Some(outcome)
}

fn is_newer(candidate: &Option<LimitSnapshot>, current: &Option<LimitSnapshot>) -> bool {
    match (candidate, current) {
        (Some(_), None) => true,
        (Some(candidate), Some(current)) => candidate.fetched_at > current.fetched_at,
        _ => false,
    }
}

fn memo() -> &'static Mutex<HashMap<String, MemoEntry>> {
    MEMO.get_or_init(|| Mutex::new(HashMap::new()))
}

fn account_lock(key: &str) -> Arc<Mutex<()>> {
    LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
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
