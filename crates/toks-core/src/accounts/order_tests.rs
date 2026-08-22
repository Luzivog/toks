use std::fs;

use crate::limits::{LimitSnapshot, Provider, SnapshotStatus};

use super::order::{apply_order, load_order, reorder_to, save_order};
use super::{
    AccountId, AccountIdentityKind, AccountOrderKey, AccountSource, CredentialProfileId,
    CredentialProfileKind, ProviderAccount,
};

fn snapshot(provider: Provider, id: &str, email: &str) -> LimitSnapshot {
    LimitSnapshot {
        provider,
        account: ProviderAccount {
            id: AccountId::new(format!("logical-{id}")),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: Some(email.into()),
            sources: vec![AccountSource {
                profile_id: CredentialProfileId::new(id),
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
        fetched_at: None,
        source: String::new(),
        issue: None,
        status: SnapshotStatus::default(),
    }
}

fn ids(snapshots: &[LimitSnapshot]) -> Vec<&str> {
    snapshots
        .iter()
        .map(|snapshot| snapshot.account.id.as_str())
        .collect()
}

#[test]
fn saved_order_is_global_and_unlisted_accounts_are_deterministic() {
    let mut snapshots = vec![
        snapshot(Provider::Codex, "z", "z@example.com"),
        snapshot(Provider::Claude, "b", "b@example.com"),
        snapshot(Provider::Claude, "a", "a@example.com"),
    ];
    apply_order(
        &mut snapshots,
        &[AccountOrderKey::new(Provider::Codex, "z")],
    );
    assert_eq!(ids(&snapshots), ["logical-z", "logical-a", "logical-b"]);
}

#[test]
fn legacy_profile_order_still_ranks_logical_accounts() {
    let mut snapshots = vec![
        snapshot(Provider::Codex, "second-profile", "second@example.com"),
        snapshot(Provider::Codex, "first-profile", "first@example.com"),
    ];

    apply_order(
        &mut snapshots,
        &[AccountOrderKey::new(Provider::Codex, "first-profile")],
    );

    assert_eq!(
        ids(&snapshots),
        ["logical-first-profile", "logical-second-profile"]
    );
}

#[test]
fn persistence_contains_only_versioned_stable_keys() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("account-order.json");
    let keys = vec![
        AccountOrderKey::new(Provider::Codex, "local-profile"),
        AccountOrderKey::new(Provider::Claude, "other-profile"),
    ];
    save_order(&path, &keys).unwrap();
    save_order(&path, &keys).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains('@'));
    assert_eq!(load_order(&path).unwrap(), keys);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn duplicate_or_empty_stored_keys_are_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("account-order.json");
    fs::write(
        &path,
        r#"{"version":1,"accounts":[{"provider":"codex","accountId":"a"},{"provider":"codex","accountId":"a"},{"provider":"claude","accountId":""}]}"#,
    )
    .unwrap();

    assert_eq!(
        load_order(&path).unwrap(),
        [AccountOrderKey::new(Provider::Codex, "a")]
    );
}

#[test]
fn dropping_on_an_account_moves_to_that_accounts_position() {
    let a = AccountOrderKey::new(Provider::Codex, "a");
    let b = AccountOrderKey::new(Provider::Claude, "b");
    let c = AccountOrderKey::new(Provider::Codex, "c");
    let mut keys = vec![a.clone(), b.clone(), c.clone()];

    assert!(reorder_to(&mut keys, &c, &a));
    assert_eq!(keys, [c.clone(), a.clone(), b.clone()]);
    assert!(reorder_to(&mut keys, &c, &b));
    assert_eq!(keys, [a, b, c]);
}
