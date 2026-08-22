use chrono::{TimeZone, Utc};
use toks_core::{
    accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind},
    limits::{LimitIssue, LimitIssueKind, SnapshotFreshness, SnapshotStatus},
    LimitSnapshot, Provider, ProviderAccount,
};

use super::AccountOperations;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0)
        .single()
        .expect("valid fixture time")
}

fn snapshot(
    provider: Provider,
    account_id: &str,
    email: Option<&str>,
    freshness: SnapshotFreshness,
    attempted_at: Option<chrono::DateTime<Utc>>,
) -> LimitSnapshot {
    LimitSnapshot {
        provider,
        account: ProviderAccount {
            id: account_id.into(),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: email.map(str::to_owned),
            sources: vec![AccountSource {
                profile_id: account_id.into(),
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
        fetched_at: attempted_at,
        source: String::new(),
        issue: None,
        status: SnapshotStatus {
            freshness,
            last_attempted_at: attempted_at,
            issue: None,
        },
    }
}

#[test]
fn add_completes_when_the_exact_account_gains_an_email() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    operations.start_add(
        Provider::Codex,
        "new-codex".into(),
        started_at,
        &mut Vec::new(),
    );
    let mut snapshots = vec![snapshot(
        Provider::Codex,
        "new-codex",
        Some("person@example.test"),
        SnapshotFreshness::Cached,
        Some(started_at - chrono::Duration::minutes(1)),
    )];

    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(15));

    assert!(operations.pending.is_empty());
    assert!(operations.errors().is_empty());
    assert_eq!(snapshots[0].status.freshness, SnapshotFreshness::Cached);
}

#[test]
fn cached_snapshot_cannot_complete_reauthentication() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    operations.start_reauthentication(Provider::Claude, "claude-a".into(), started_at);
    let mut snapshots = vec![snapshot(
        Provider::Claude,
        "claude-a",
        Some("person@example.test"),
        SnapshotFreshness::Cached,
        Some(started_at - chrono::Duration::minutes(1)),
    )];

    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(15));

    assert_eq!(operations.pending.len(), 1);
    assert_eq!(snapshots[0].status.freshness, SnapshotFreshness::Loading);
}

#[test]
fn only_a_post_start_live_attempt_completes_reauthentication() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    operations.start_reauthentication(Provider::Codex, "codex-a".into(), started_at);
    let mut snapshots = vec![snapshot(
        Provider::Codex,
        "codex-a",
        Some("person@example.test"),
        SnapshotFreshness::Live,
        Some(started_at - chrono::Duration::seconds(1)),
    )];
    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(15));
    assert_eq!(operations.pending.len(), 1);

    snapshots[0] = snapshot(
        Provider::Codex,
        "codex-a",
        Some("person@example.test"),
        SnapshotFreshness::Live,
        Some(started_at + chrono::Duration::seconds(20)),
    );
    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(20));
    assert!(operations.pending.is_empty());
}

#[test]
fn concurrent_provider_operations_reconcile_independently() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    operations.start_add(
        Provider::Claude,
        "claude-new".into(),
        started_at,
        &mut Vec::new(),
    );
    operations.start_reauthentication(Provider::Codex, "codex-old".into(), started_at);
    let mut snapshots = vec![
        snapshot(
            Provider::Claude,
            "claude-new",
            Some("claude@example.test"),
            SnapshotFreshness::Loading,
            None,
        ),
        snapshot(
            Provider::Codex,
            "codex-old",
            Some("codex@example.test"),
            SnapshotFreshness::Cached,
            Some(started_at - chrono::Duration::minutes(1)),
        ),
    ];

    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(15));

    assert_eq!(operations.pending.len(), 1);
    assert_eq!(operations.pending[0].key.profile_id.as_str(), "codex-old");
}

#[test]
fn fresh_authentication_failure_finishes_with_a_dismissible_error() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    operations.start_reauthentication(Provider::Claude, "claude-a".into(), started_at);
    let attempted_at = started_at + chrono::Duration::seconds(10);
    let mut failed = snapshot(
        Provider::Claude,
        "claude-a",
        Some("person@example.test"),
        SnapshotFreshness::Cached,
        Some(attempted_at),
    );
    failed.status.issue = Some(LimitIssue {
        kind: LimitIssueKind::Authentication,
        message: "unauthorized".into(),
        attempted_at,
        retry_at: None,
    });

    operations.reconcile(
        std::slice::from_mut(&mut failed),
        started_at + chrono::Duration::seconds(15),
    );

    assert!(operations.pending.is_empty());
    assert_eq!(operations.errors().len(), 1);
    let error_id = operations.errors()[0].id;
    operations.dismiss_error(error_id);
    assert!(operations.errors().is_empty());
}

#[test]
fn transient_provider_failures_are_not_presented_as_authentication_failures() {
    let started_at = now();
    for kind in [LimitIssueKind::Network, LimitIssueKind::RateLimited] {
        let mut operations = AccountOperations::default();
        operations.start_reauthentication(Provider::Codex, "codex-a".into(), started_at);
        let attempted_at = started_at + chrono::Duration::seconds(10);
        let mut failed = snapshot(
            Provider::Codex,
            "codex-a",
            Some("person@example.test"),
            SnapshotFreshness::Cached,
            Some(attempted_at),
        );
        failed.status.issue = Some(LimitIssue {
            kind,
            message: "temporary".into(),
            attempted_at,
            retry_at: None,
        });

        operations.reconcile(
            std::slice::from_mut(&mut failed),
            started_at + chrono::Duration::seconds(15),
        );

        assert_eq!(operations.pending.len(), 1);
        assert!(operations.errors().is_empty());
        assert_eq!(
            failed.status.issue.as_ref().map(|issue| issue.kind),
            Some(kind)
        );
    }
}

#[test]
fn pending_sign_in_times_out_with_a_dismissible_error() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    let mut snapshots = Vec::new();
    operations.start_add(
        Provider::Codex,
        "codex-new".into(),
        started_at,
        &mut snapshots,
    );

    operations.reconcile(&mut snapshots, started_at + chrono::Duration::minutes(6));

    assert!(operations.pending.is_empty());
    assert_eq!(operations.errors().len(), 1);
    assert!(operations.errors()[0].message.contains("Couldn't confirm"));
}

#[test]
fn disappearing_account_reports_cancellation_without_affecting_other_operations() {
    let started_at = now();
    let mut operations = AccountOperations::default();
    operations.start_add(
        Provider::Claude,
        "claude-new".into(),
        started_at,
        &mut Vec::new(),
    );
    operations.start_reauthentication(Provider::Codex, "codex-old".into(), started_at);
    let mut snapshots = vec![snapshot(
        Provider::Claude,
        "claude-new",
        None,
        SnapshotFreshness::Loading,
        None,
    )];
    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(15));
    snapshots.clear();

    operations.reconcile(&mut snapshots, started_at + chrono::Duration::seconds(30));

    assert!(operations.pending.is_empty());
    assert_eq!(operations.errors().len(), 2);
    assert!(operations
        .errors()
        .iter()
        .all(|error| error.message.contains("cancelled")));
}
