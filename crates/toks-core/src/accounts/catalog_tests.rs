use chrono::{TimeZone, Utc};

use crate::limits::{LimitSnapshot, LimitWindow, Provider};

use super::{
    coalesce_snapshots, AccountBinding, AccountId, AccountIdentityKind, AccountOrigin,
    AccountSource, CredentialProfileId, CredentialProfileKind, ProviderAccount,
};

fn snapshot(
    logical_id: &str,
    profile_id: &str,
    kind: CredentialProfileKind,
    email: &str,
    fetched_at: i64,
    percent_used: f64,
) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: AccountId::new(logical_id),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some(email.into()),
            sources: vec![AccountSource {
                profile_id: CredentialProfileId::new(profile_id),
                kind,
                primary: true,
            }],
        },
        plan: Some("pro".into()),
        plan_multiplier: None,
        windows: vec![LimitWindow {
            id: "weekly".into(),
            label: "Weekly".into(),
            percent_used,
            resets_at: None,
            severity: None,
            scope: None,
            is_active: true,
            raw: serde_json::Value::Null,
        }],
        extras: Vec::new(),
        fetched_at: Utc.timestamp_opt(fetched_at, 0).single(),
        source: profile_id.into(),
        issue: None,
        status: Default::default(),
    }
}

#[test]
fn verified_same_principal_coalesces_and_uses_freshest_snapshot() {
    let current = snapshot(
        "codex-principal",
        "codex-current",
        CredentialProfileKind::Current,
        "old@example.com",
        10,
        12.0,
    );
    let managed = snapshot(
        "codex-principal",
        "managed-1",
        CredentialProfileKind::Managed,
        "fresh@example.com",
        20,
        78.0,
    );

    let snapshots = coalesce_snapshots(vec![(0, current), (1, managed)]);

    assert_eq!(snapshots.len(), 1);
    let account = &snapshots[0].account;
    assert_eq!(snapshots[0].windows[0].percent_used, 78.0);
    assert_eq!(account.email.as_deref(), Some("fresh@example.com"));
    assert_eq!(account.origin(), AccountOrigin::Mixed);
    assert_eq!(account.sources.len(), 2);
    assert_eq!(
        account
            .primary_source()
            .map(|source| source.profile_id.as_str()),
        Some("managed-1")
    );
}

#[test]
fn same_email_with_different_principals_stays_distinct() {
    let first = snapshot(
        "codex-first",
        "first-profile",
        CredentialProfileKind::Managed,
        "same@example.com",
        10,
        12.0,
    );
    let second = snapshot(
        "codex-second",
        "second-profile",
        CredentialProfileKind::Managed,
        "same@example.com",
        20,
        78.0,
    );

    let snapshots = coalesce_snapshots(vec![(0, first), (1, second)]);

    assert_eq!(snapshots.len(), 2);
    assert_ne!(snapshots[0].account.id, snapshots[1].account.id);
}

#[test]
fn duplicate_source_descriptors_are_not_repeated() {
    let first = snapshot(
        "codex-principal",
        "managed-1",
        CredentialProfileKind::Managed,
        "same@example.com",
        10,
        12.0,
    );
    let second = snapshot(
        "codex-principal",
        "managed-1",
        CredentialProfileKind::Managed,
        "same@example.com",
        20,
        78.0,
    );

    let snapshots = coalesce_snapshots(vec![(0, first), (1, second)]);

    assert_eq!(snapshots[0].account.sources.len(), 1);
    assert!(snapshots[0].account.sources[0].primary);
}

#[test]
fn binding_transition_requires_same_local_profile_and_new_logical_account() {
    let binding = |profile: &str, account: &str| AccountBinding {
        provider: Provider::Codex,
        profile_id: CredentialProfileId::new(profile),
        account_id: AccountId::new(account),
    };
    let previous = binding("codex-current", "codex-first");
    let current = binding("codex-current", "codex-second");

    let transition = previous.transition_to(&current).unwrap();

    assert_eq!(transition.profile_id.as_str(), "codex-current");
    assert_eq!(transition.previous_account_id.as_str(), "codex-first");
    assert_eq!(transition.account_id.as_str(), "codex-second");
    assert!(previous
        .transition_to(&binding("managed-1", "codex-second"))
        .is_none());
}
