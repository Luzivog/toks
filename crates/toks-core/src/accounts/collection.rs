use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::limits::{self, LimitIssue, LimitSnapshot, Provider, SnapshotStatus};

use super::{
    account_email, coalesce_snapshots, discover_profiles, filter_hidden_accounts, AccountProfile,
};

const MAX_PARALLEL_ACCOUNTS: usize = 4;

#[derive(Clone, Copy)]
enum CollectionMode {
    Hydrate,
    Refresh,
}

/// Refresh every discovered account with bounded parallelism. Broken accounts
/// retain their last successful snapshot and an account-local typed issue.
pub fn collect_limits() -> Vec<LimitSnapshot> {
    collect(CollectionMode::Refresh)
}

/// Read local snapshots only. This is intentionally separate from refresh so
/// applications can paint last-known values before any network work starts.
pub fn hydrate_limits() -> Vec<LimitSnapshot> {
    collect(CollectionMode::Hydrate)
}

fn collect(mode: CollectionMode) -> Vec<LimitSnapshot> {
    let profiles = discover_profiles();
    if profiles.is_empty() {
        return Vec::new();
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
    filter_hidden_accounts(coalesce_snapshots(results))
}

fn collect_profile(profile: &AccountProfile, mode: CollectionMode) -> LimitSnapshot {
    let outcome = match mode {
        CollectionMode::Hydrate => limits::live::RefreshOutcome {
            snapshot: limits::live::hydrate(profile),
            issue: None,
        },
        CollectionMode::Refresh => limits::live::refresh(profile),
    };
    match outcome.snapshot {
        Some(snapshot) => finish_snapshot(snapshot, profile),
        None => unavailable_snapshot(profile, outcome.issue),
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
    let freshness = state.freshness;
    let status = issue.clone().map_or_else(
        || SnapshotStatus::at(freshness),
        |issue| SnapshotStatus::failed(freshness, issue),
    );
    let legacy_issue = issue.as_ref().map(|problem| problem.message.clone());
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
        issue: legacy_issue,
        status,
    }
}

fn plan_details(profile: &AccountProfile) -> limits::PlanDetails {
    match profile.provider {
        Provider::Claude => limits::read_claude_plan(&profile.config_dir),
        Provider::Codex => limits::codex::read_plan_from_auth(&profile.config_dir),
    }
}
