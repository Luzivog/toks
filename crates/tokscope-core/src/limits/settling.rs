use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{LimitIssue, LimitIssueKind, SnapshotFreshness};
use crate::accounts::AccountProfile;

// Managed profiles become discoverable before provider CLIs finish their
// credential writes. Keep that expected transition out of the error UI, but
// bound it so an abandoned or invalid sign-in cannot remain loading forever.
const SIGN_IN_SETTLING_WINDOW: Duration = Duration::from_secs(5 * 60);

pub(crate) struct MissingSnapshotState {
    pub(crate) freshness: SnapshotFreshness,
    pub(crate) issue: Option<LimitIssue>,
}

pub(crate) fn missing_snapshot_state(
    profile: &AccountProfile,
    credentials_present: bool,
    refresh_issue: Option<LimitIssue>,
) -> MissingSnapshotState {
    missing_snapshot_state_at(
        profile,
        credentials_present,
        refresh_issue,
        unix_millis(SystemTime::now()),
    )
}

fn missing_snapshot_state_at(
    profile: &AccountProfile,
    credentials_present: bool,
    refresh_issue: Option<LimitIssue>,
    now_ms: u128,
) -> MissingSnapshotState {
    if is_settling(profile, now_ms) {
        return MissingSnapshotState {
            freshness: SnapshotFreshness::Loading,
            issue: None,
        };
    }

    let issue = refresh_issue.or_else(|| {
        (!credentials_present).then(|| {
            LimitIssue::new(
                LimitIssueKind::Authentication,
                "Sign in with this provider to see plan limits.",
            )
        })
    });
    let freshness = if credentials_present && issue.is_none() {
        SnapshotFreshness::Loading
    } else {
        SnapshotFreshness::Unavailable
    };
    MissingSnapshotState { freshness, issue }
}

pub(crate) fn is_settling(profile: &AccountProfile, now_ms: u128) -> bool {
    let Some(created_at_ms) = profile.created_at_ms.filter(|_| profile.managed) else {
        return false;
    };
    let age_ms = now_ms.saturating_sub(created_at_ms);
    age_ms <= SIGN_IN_SETTLING_WINDOW.as_millis()
}

pub(crate) fn transient_auth_failure(profile: &AccountProfile, issue: &LimitIssue) -> bool {
    transient_auth_failure_at(profile, issue, unix_millis(SystemTime::now()))
}

fn transient_auth_failure_at(profile: &AccountProfile, issue: &LimitIssue, now_ms: u128) -> bool {
    issue.kind == LimitIssueKind::Authentication && is_settling(profile, now_ms)
}

fn unix_millis(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
pub(super) fn missing_snapshot_state_for_test(
    profile: &AccountProfile,
    credentials_present: bool,
    refresh_issue: Option<LimitIssue>,
    now_ms: u128,
) -> MissingSnapshotState {
    missing_snapshot_state_at(profile, credentials_present, refresh_issue, now_ms)
}

#[cfg(test)]
pub(super) fn transient_auth_failure_for_test(
    profile: &AccountProfile,
    issue: &LimitIssue,
    now_ms: u128,
) -> bool {
    transient_auth_failure_at(profile, issue, now_ms)
}
