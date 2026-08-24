use chrono::{DateTime, Utc};
use toks_core::accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind};
use toks_core::limits::{
    BankedResetCredit, BankedResetCreditStatus, LimitIssue, LimitIssueKind, LimitWindow,
    PlanMultiplier, SnapshotFreshness, SnapshotStatus,
};
use toks_core::{LimitSnapshot, Provider, ProviderAccount};

use super::fixture_now;

pub(crate) fn limit_snapshot(provider: Provider, id: &str) -> LimitSnapshot {
    let now = fixture_now();
    let mut snapshot = live_snapshot(provider, id, now);
    snapshot.windows = vec![limit_window("weekly", 42.0, now)];
    snapshot.windows[0].label = "Weekly — GPT-5.3-Codex-Spark".into();
    snapshot.windows[0].scope = Some("GPT-5.3-Codex-Spark".into());
    snapshot.fetched_at = Some(now - chrono::Duration::minutes(2));
    snapshot
}

pub(crate) fn account_removal_snapshot(id: &str, email: &str, now: DateTime<Utc>) -> LimitSnapshot {
    let mut snapshot = live_snapshot(Provider::Codex, id, now);
    snapshot.account.email = Some(email.into());
    snapshot.plan = Some("Pro".into());
    snapshot
}

pub(crate) fn privacy_snapshot(now: DateTime<Utc>) -> LimitSnapshot {
    let mut snapshot = live_snapshot(Provider::Codex, "privacy", now);
    snapshot.account.identity_kind = AccountIdentityKind::ProfileFallback;
    snapshot.account.email = Some("hello@example.test".into());
    snapshot.account.sources[0].profile_id = "privacy-source".into();
    snapshot.plan = Some("Pro".into());
    snapshot
}

pub(crate) fn banked_reset_snapshot(
    provider: Provider,
    id: &str,
    banked_resets: u64,
    now: DateTime<Utc>,
) -> LimitSnapshot {
    let mut snapshot = live_snapshot(provider, id, now);
    snapshot.account.email = None;
    snapshot.plan = Some("pro".into());
    snapshot.plan_multiplier = Some(PlanMultiplier::Twenty);
    snapshot.banked_resets = banked_resets;
    snapshot.banked_reset_credits = (provider == Provider::Codex && id == "positive").then(|| {
        vec![
            BankedResetCredit {
                expires_at: Some(now + chrono::Duration::hours(1)),
                title: Some("Redeemed reset".into()),
                status: Some(BankedResetCreditStatus::Redeemed),
            },
            BankedResetCredit {
                expires_at: Some(now + chrono::Duration::days(2)),
                title: Some("Later reset".into()),
                status: Some(BankedResetCreditStatus::Available),
            },
            BankedResetCredit {
                expires_at: Some(now + chrono::Duration::days(1)),
                title: Some("Earlier reset".into()),
                status: Some(BankedResetCreditStatus::Available),
            },
        ]
    });
    snapshot.windows = (id == "positive")
        .then(|| limit_window("weekly-positive", 42.0, now))
        .into_iter()
        .collect();
    snapshot
}

pub(crate) fn failed_snapshot(
    provider: Provider,
    id: &str,
    kind: LimitIssueKind,
    now: DateTime<Utc>,
) -> LimitSnapshot {
    let mut snapshot = live_snapshot(provider, id, now);
    snapshot.account.sources[0].profile_id = id.into();
    snapshot.plan = Some("Max".into());
    snapshot.windows = vec![limit_window(format!("weekly-{id}"), 42.0, now)];
    snapshot.fetched_at = Some(now - chrono::Duration::minutes(2));
    snapshot.source = "cache".into();
    snapshot.status = SnapshotStatus {
        freshness: SnapshotFreshness::Cached,
        last_attempted_at: Some(now),
        issue: Some(LimitIssue {
            kind,
            message: "failed".into(),
            attempted_at: now,
            retry_at: None,
        }),
    };
    snapshot
}

pub(crate) fn remote_control_snapshot(now: DateTime<Utc>) -> LimitSnapshot {
    let mut snapshot = live_snapshot(Provider::Codex, "control", now);
    snapshot.account.email = Some("hello@example.test".into());
    snapshot.account.sources[0].profile_id = "control-profile".into();
    snapshot.account.sources[0].kind = CredentialProfileKind::Current;
    snapshot.source = "fixture".into();
    snapshot
}

pub(crate) fn rotation_limit_snapshot(
    now: DateTime<Utc>,
    id: &str,
    percent_used: f64,
) -> LimitSnapshot {
    let mut snapshot = live_snapshot(Provider::Codex, id, now);
    snapshot.account.sources[0].profile_id = format!("{id}-profile").into();
    snapshot.windows = vec![limit_window(format!("weekly-{id}"), percent_used, now)];
    snapshot.source = "fixture".into();
    snapshot
}

fn live_snapshot(provider: Provider, id: &str, now: DateTime<Utc>) -> LimitSnapshot {
    LimitSnapshot {
        provider,
        account: ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some(format!("{id}@example.test")),
            sources: vec![AccountSource {
                profile_id: format!("profile-{id}").into(),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
        plan: None,
        plan_multiplier: None,
        banked_resets: 0,
        banked_reset_credits: None,
        windows: Vec::new(),
        extras: Vec::new(),
        fetched_at: Some(now),
        source: String::new(),
        issue: None,
        status: SnapshotStatus {
            freshness: SnapshotFreshness::Live,
            last_attempted_at: Some(now),
            issue: None,
        },
    }
}

fn limit_window(id: impl Into<String>, percent_used: f64, now: DateTime<Utc>) -> LimitWindow {
    LimitWindow {
        id: id.into(),
        label: "Weekly".into(),
        percent_used,
        resets_at: Some(now + chrono::Duration::days(6)),
        severity: None,
        scope: None,
        is_active: true,
        raw: Default::default(),
    }
}
