use tokscope_core::accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind};
use tokscope_core::{LimitSnapshot, Provider, ProviderAccount};

use super::{AccountRemovals, RemovalStatus};

#[test]
fn completed_removal_filters_only_the_exact_logical_account() {
    let mut removals = AccountRemovals::default();
    let removed = key("removed");
    removals.complete(removed.clone());
    let mut snapshots = vec![snapshot("removed"), snapshot("sibling")];

    removals.filter_refresh(&mut snapshots);

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].account.id.as_str(), "sibling");
    assert_eq!(removals.status(&removed), RemovalStatus::Ready);
}

#[test]
fn explicit_readd_allows_a_previously_removed_logical_account() {
    let mut removals = AccountRemovals::default();
    let restored = key("restored");
    removals.complete(restored.clone());
    removals.allow(&restored);
    let mut snapshots = vec![snapshot("restored")];

    removals.filter_refresh(&mut snapshots);

    assert_eq!(snapshots.len(), 1);
}

#[test]
fn confirmation_is_reversible_before_filesystem_work_starts() {
    let mut removals = AccountRemovals::default();
    let account = key("confirm");
    removals.confirm(account.clone());
    assert_eq!(removals.status(&account), RemovalStatus::Confirming);
    removals.cancel_confirmation(&account);
    assert_eq!(removals.status(&account), RemovalStatus::Ready);
}

fn key(id: &str) -> tokscope_core::accounts::AccountOrderKey {
    tokscope_core::accounts::AccountOrderKey::new(Provider::Codex, id)
}

fn snapshot(id: &str) -> LimitSnapshot {
    LimitSnapshot::loading_account(
        Provider::Codex,
        ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: None,
            sources: vec![AccountSource {
                profile_id: format!("profile-{id}").into(),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
    )
}
