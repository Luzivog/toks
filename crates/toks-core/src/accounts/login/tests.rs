use crate::limits::Provider;

use super::{exact_profile, CredentialProfileId};
use crate::accounts::{AccountId, AccountIdentityKind, AccountProfile, ProviderAccount};

#[test]
fn reauthentication_resolves_only_the_exact_credential_profile() {
    let profile = |provider: Provider, id: &str| AccountProfile {
        provider,
        profile_id: CredentialProfileId::new(id),
        account: ProviderAccount {
            id: AccountId::new("shared-logical-account"),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some("same@example.com".into()),
            sources: Vec::new(),
        },
        home_dir: format!("/{id}/home").into(),
        config_dir: format!("/{id}/config").into(),
        managed: true,
        created_at_ms: Some(1),
    };
    let profiles = vec![
        profile(Provider::Claude, "first"),
        profile(Provider::Codex, "first"),
        profile(Provider::Claude, "second"),
    ];
    let second = CredentialProfileId::new("second");
    let found = exact_profile(profiles.clone(), Provider::Claude, &second).unwrap();
    assert_eq!(found.profile_id, second);
    assert_eq!(found.provider, Provider::Claude);
    assert!(exact_profile(
        profiles,
        Provider::Codex,
        &CredentialProfileId::new("missing")
    )
    .is_none());
}
