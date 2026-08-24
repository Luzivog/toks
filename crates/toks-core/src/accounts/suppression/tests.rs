use std::fs;

use crate::limits::{LimitSnapshot, Provider};

use super::store::SuppressionStore;
use super::unhide_profile_from;
use crate::accounts::{
    AccountId, AccountIdentityKind, AccountProfile, AccountSource, CredentialProfileId,
    CredentialProfileKind, ProviderAccount,
};

#[test]
fn current_hide_persists_without_provider_identity_material() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state").join("suppression.json");
    let account = account(
        "codex-opaque-a",
        AccountIdentityKind::ProviderPrincipal,
        &[current("codex-current")],
    );
    SuppressionStore::at(path.clone())
        .hide(Provider::Codex, &account)
        .unwrap();

    let visible =
        SuppressionStore::at(path.clone()).filter(vec![snapshot(Provider::Codex, account)]);
    assert!(visible.is_empty());
    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains('@'));
    assert!(!raw.contains("provider-account-subject"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}

#[test]
fn provider_principal_transition_shows_the_new_account_and_releases_alias() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("suppression.json");
    let store = SuppressionStore::at(path.clone());
    store
        .hide(
            Provider::Codex,
            &account(
                "codex-opaque-a",
                AccountIdentityKind::ProviderPrincipal,
                &[current("codex-current")],
            ),
        )
        .unwrap();

    let transitioned = snapshot(
        Provider::Codex,
        account(
            "codex-opaque-b",
            AccountIdentityKind::ProviderPrincipal,
            &[current("codex-current")],
        ),
    );
    assert_eq!(store.filter(vec![transitioned]).len(), 1);

    let fallback = snapshot(
        Provider::Codex,
        account(
            "codex-profile-codex-current",
            AccountIdentityKind::ProfileFallback,
            &[current("codex-current")],
        ),
    );
    assert_eq!(SuppressionStore::at(path).filter(vec![fallback]).len(), 1);
}

#[test]
fn missing_principal_fallback_for_hidden_current_profile_stays_hidden() {
    let temp = tempfile::tempdir().unwrap();
    let store = SuppressionStore::at(temp.path().join("suppression.json"));
    store
        .hide(
            Provider::Claude,
            &account(
                "claude-opaque-a",
                AccountIdentityKind::ProviderPrincipal,
                &[current("claude-current")],
            ),
        )
        .unwrap();

    let fallback = snapshot(
        Provider::Claude,
        account(
            "claude-profile-claude-current",
            AccountIdentityKind::ProfileFallback,
            &[current("claude-current")],
        ),
    );
    assert!(store.filter(vec![fallback]).is_empty());
}

#[test]
fn successful_managed_add_explicitly_unhides_the_same_principal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("suppression.json");
    let store = SuppressionStore::at(path.clone());
    store
        .hide(
            Provider::Codex,
            &account(
                "codex-opaque-a",
                AccountIdentityKind::ProviderPrincipal,
                &[current("codex-current")],
            ),
        )
        .unwrap();

    assert!(store
        .unhide(Provider::Codex, &AccountId::new("codex-opaque-a"))
        .unwrap());

    let readded = snapshot(
        Provider::Codex,
        account(
            "codex-opaque-a",
            AccountIdentityKind::ProviderPrincipal,
            &[current("codex-current"), managed("new-managed")],
        ),
    );
    assert_eq!(store.filter(vec![readded]).len(), 1);

    let current_only = snapshot(
        Provider::Codex,
        account(
            "codex-opaque-a",
            AccountIdentityKind::ProviderPrincipal,
            &[current("codex-current")],
        ),
    );
    assert_eq!(
        SuppressionStore::at(path).filter(vec![current_only]).len(),
        1
    );
}

#[test]
fn added_profile_resolves_principal_before_unhiding() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("suppression.json");
    let store = SuppressionStore::at(path.clone());
    let hidden = account(
        "codex-opaque-a",
        AccountIdentityKind::ProviderPrincipal,
        &[current("codex-current")],
    );
    store.hide(Provider::Codex, &hidden).unwrap();
    let profile_id = CredentialProfileId::new("managed-new");
    let profile = profile(
        Provider::Codex,
        profile_id.clone(),
        account(
            "codex-opaque-a",
            AccountIdentityKind::ProviderPrincipal,
            &[managed("managed-new")],
        ),
    );

    let resolved = unhide_profile_from(&store, Provider::Codex, &profile_id, vec![profile])
        .unwrap()
        .unwrap();
    assert_eq!(resolved, AccountId::new("codex-opaque-a"));
    assert_eq!(
        SuppressionStore::at(path)
            .filter(vec![snapshot(Provider::Codex, hidden)])
            .len(),
        1
    );
}

#[test]
fn fallback_profile_cannot_unhide_a_logical_account() {
    let temp = tempfile::tempdir().unwrap();
    let store = SuppressionStore::at(temp.path().join("suppression.json"));
    let hidden = account(
        "codex-opaque-a",
        AccountIdentityKind::ProviderPrincipal,
        &[current("codex-current")],
    );
    store.hide(Provider::Codex, &hidden).unwrap();
    let profile_id = CredentialProfileId::new("managed-new");
    let profile = profile(
        Provider::Codex,
        profile_id.clone(),
        account(
            "codex-profile-managed-new",
            AccountIdentityKind::ProfileFallback,
            &[managed("managed-new")],
        ),
    );

    assert!(
        unhide_profile_from(&store, Provider::Codex, &profile_id, vec![profile])
            .unwrap()
            .is_none()
    );
    assert!(store
        .filter(vec![snapshot(Provider::Codex, hidden)])
        .is_empty());
}

fn account(
    id: &str,
    identity_kind: AccountIdentityKind,
    sources: &[AccountSource],
) -> ProviderAccount {
    ProviderAccount {
        id: AccountId::new(id),
        identity_kind,
        email: None,
        sources: sources.to_vec(),
    }
}

fn current(id: &str) -> AccountSource {
    source(id, CredentialProfileKind::Current)
}

fn managed(id: &str) -> AccountSource {
    source(id, CredentialProfileKind::Managed)
}

fn source(id: &str, kind: CredentialProfileKind) -> AccountSource {
    AccountSource {
        profile_id: CredentialProfileId::new(id),
        kind,
        primary: true,
    }
}

fn snapshot(provider: Provider, account: ProviderAccount) -> LimitSnapshot {
    LimitSnapshot::loading_account(provider, account)
}

fn profile(
    provider: Provider,
    profile_id: CredentialProfileId,
    account: ProviderAccount,
) -> AccountProfile {
    AccountProfile {
        provider,
        profile_id,
        account,
        home_dir: "/unused/home".into(),
        config_dir: "/unused/config".into(),
        managed: true,
        created_at_ms: Some(1),
    }
}
