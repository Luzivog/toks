use chrono::{DateTime, Utc};
use toks_core::{
    limits::{LimitIssueKind, SnapshotFreshness},
    LimitSnapshot,
};

use super::{OperationKind, PendingOperation, SIGN_IN_TIMEOUT};

pub(super) enum Outcome {
    Pending,
    Complete,
    Failed(String),
}

pub(super) fn outcome(
    pending: &PendingOperation,
    snapshot: Option<&LimitSnapshot>,
    now: DateTime<Utc>,
) -> Outcome {
    let provider = pending.key.provider.display_name();
    let Some(snapshot) = snapshot else {
        if pending.observed || pending.missing_refreshes >= 2 {
            return Outcome::Failed(format!("{provider} sign-in was cancelled."));
        }
        return timed_out(pending, now, provider);
    };
    let attempted_after_start = attempted_after(snapshot, pending.started_at);
    if pending.kind == OperationKind::Add && snapshot.account.email.is_some()
        || snapshot.status.freshness == SnapshotFreshness::Live && attempted_after_start
    {
        return Outcome::Complete;
    }
    if snapshot
        .status
        .issue
        .as_ref()
        .is_some_and(|issue| issue.kind == LimitIssueKind::Authentication && attempted_after_start)
    {
        return Outcome::Failed(format!("{provider} sign-in wasn't completed."));
    }
    timed_out(pending, now, provider)
}

fn timed_out(pending: &PendingOperation, now: DateTime<Utc>, provider: &str) -> Outcome {
    if now.signed_duration_since(pending.started_at) >= SIGN_IN_TIMEOUT {
        Outcome::Failed(format!("Couldn't confirm {provider} sign-in. Try again."))
    } else {
        Outcome::Pending
    }
}

fn attempted_after(snapshot: &LimitSnapshot, started_at: DateTime<Utc>) -> bool {
    snapshot
        .status
        .last_attempted_at
        .is_some_and(|attempted_at| attempted_at > started_at)
        || snapshot
            .status
            .issue
            .as_ref()
            .is_some_and(|issue| issue.attempted_at > started_at)
}

pub(super) fn mark_pending(snapshot: &mut LimitSnapshot, started_at: DateTime<Utc>) {
    let transient_issue = snapshot.status.issue.as_ref().is_some_and(|issue| {
        matches!(
            issue.kind,
            LimitIssueKind::Network | LimitIssueKind::RateLimited
        ) && issue.attempted_at > started_at
    });
    if !transient_issue {
        snapshot.status.freshness = SnapshotFreshness::Loading;
        snapshot.status.issue = None;
    }
}
