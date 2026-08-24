use crate::limits::{self, LimitIssue, LimitSnapshot, Provider, SnapshotStatus};

use super::super::{account_email, AccountProfile, CodexAuthProof};

pub(super) struct CollectedProfile {
    pub(super) snapshot: LimitSnapshot,
    pub(super) codex_auth: Option<CodexAuthProof>,
}

pub(super) fn finish_outcome(
    profile: &AccountProfile,
    outcome: limits::live::RefreshOutcome,
) -> CollectedProfile {
    match outcome.snapshot {
        Some(snapshot)
            if outcome
                .codex_auth
                .as_ref()
                .is_none_or(|proof| proof.is_current(profile)) =>
        {
            CollectedProfile {
                snapshot: finish_snapshot(snapshot, profile),
                codex_auth: outcome.codex_auth,
            }
        }
        None => CollectedProfile {
            snapshot: unavailable_snapshot(profile, outcome.issue),
            codex_auth: None,
        },
        Some(_) => CollectedProfile {
            snapshot: unavailable_snapshot(
                profile,
                Some(LimitIssue::new(
                    crate::limits::LimitIssueKind::Authentication,
                    "Codex credentials changed before usage could be published",
                )),
            ),
            codex_auth: None,
        },
    }
}

fn finish_snapshot(mut snapshot: LimitSnapshot, profile: &AccountProfile) -> LimitSnapshot {
    let mut account = profile.account.clone();
    if account.email.is_none() {
        account.email = snapshot
            .account
            .email
            .or_else(|| account_email(profile.provider, &profile.home_dir, &profile.config_dir));
    }
    snapshot.account = account;
    if snapshot.plan.is_none() || snapshot.plan_multiplier.is_none() {
        let details = plan_details(profile);
        snapshot.plan = snapshot.plan.or(details.name);
        snapshot.plan_multiplier = snapshot.plan_multiplier.or(details.multiplier);
    }
    snapshot.issue = None;
    snapshot
}

fn unavailable_snapshot(
    profile: &AccountProfile,
    refresh_issue: Option<LimitIssue>,
) -> LimitSnapshot {
    let credentials = limits::live::credentials_present(profile);
    let state = limits::settling::missing_snapshot_state(profile, credentials, refresh_issue);
    let issue = state.issue;
    let status = issue.clone().map_or_else(
        || SnapshotStatus::at(state.freshness),
        |issue| SnapshotStatus::failed(state.freshness, issue),
    );
    let details = plan_details(profile);
    LimitSnapshot {
        provider: profile.provider,
        account: profile.account.clone(),
        plan: details.name,
        plan_multiplier: details.multiplier,
        banked_resets: 0,
        banked_reset_credits: None,
        windows: Vec::new(),
        extras: Vec::new(),
        fetched_at: None,
        source: String::new(),
        issue: issue.as_ref().map(|problem| problem.message.clone()),
        status,
    }
}

fn plan_details(profile: &AccountProfile) -> limits::PlanDetails {
    match profile.provider {
        Provider::Claude => limits::read_claude_plan(&profile.config_dir),
        Provider::Codex => limits::codex::read_plan_from_auth(&profile.config_dir),
    }
}
