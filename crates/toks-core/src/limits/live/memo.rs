//! Process-local refresh memoization and per-profile synchronization.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::RefreshOutcome;
use crate::accounts::CredentialProfileId;
use crate::limits::{credentials::CredentialRevision, LimitSnapshot};
use crate::Provider;

#[derive(Clone)]
struct MemoEntry {
    attempted_at: Instant,
    outcome: RefreshOutcome,
    failures: u32,
    retry_for: Duration,
    credential_revision: Option<CredentialRevision>,
}

type AccountLocks = HashMap<String, Arc<Mutex<()>>>;

static MEMO: OnceLock<Mutex<HashMap<String, MemoEntry>>> = OnceLock::new();
static LOCKS: OnceLock<Mutex<AccountLocks>> = OnceLock::new();

pub(super) fn get(
    key: &str,
    baseline: Option<LimitSnapshot>,
    credential_revision: Option<CredentialRevision>,
) -> Option<RefreshOutcome> {
    let mut entries = memo().lock().unwrap_or_else(|poison| poison.into_inner());
    let entry = entries.get(key)?;
    if entry.credential_revision != credential_revision
        || entry.attempted_at.elapsed() >= entry.retry_for
    {
        return None;
    }

    if is_newer(&baseline, &entry.outcome.snapshot) {
        entries.remove(key);
        return None;
    }

    Some(entry.outcome.clone())
}

pub(super) fn previous_failures(key: &str) -> u32 {
    memo()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(key)
        .map(|entry| entry.failures)
        .unwrap_or(0)
}

pub(super) fn remember(
    key: String,
    outcome: RefreshOutcome,
    failures: u32,
    retry_for: Duration,
    credential_revision: Option<CredentialRevision>,
) {
    memo()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(
            key,
            MemoEntry {
                attempted_at: Instant::now(),
                outcome,
                failures,
                retry_for,
                credential_revision,
            },
        );
}

pub(super) fn account_lock(key: &str) -> Arc<Mutex<()>> {
    LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub(super) fn forget(provider: Provider, profile_id: &CredentialProfileId) {
    let key = format!("{}:{}", provider.slug(), profile_id);
    memo()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&key);
    if let Some(locks) = LOCKS.get() {
        locks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&key);
    }
}

fn memo() -> &'static Mutex<HashMap<String, MemoEntry>> {
    MEMO.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_newer(candidate: &Option<LimitSnapshot>, current: &Option<LimitSnapshot>) -> bool {
    match (candidate, current) {
        (Some(_), None) => true,
        (Some(candidate), Some(current)) => candidate.fetched_at > current.fetched_at,
        _ => false,
    }
}
