use std::fs;

use crate::limits::{self, Provider};
use base64::Engine as _;

use super::discovery::{discover_managed_profiles, retain_unique_profiles};
use super::login::exact_profile;
use super::{write_metadata, AccountProfile, ProfileMetadata, ProviderAccount, PROFILE_VERSION};

#[test]
fn discovers_every_managed_account_in_creation_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("codex");
    fs::create_dir_all(&root).unwrap();
    for (id, created_at_ms) in [("later", 20), ("first", 10)] {
        let profile = root.join(id);
        fs::create_dir_all(profile.join("home").join(".codex")).unwrap();
        write_metadata(
            &profile.join("profile.json"),
            &ProfileMetadata {
                version: PROFILE_VERSION,
                id: id.to_string(),
                provider: Provider::Codex,
                created_at_ms,
            },
        )
        .unwrap();
    }

    let profiles = discover_managed_profiles(&root, Provider::Codex);
    let ids: Vec<&str> = profiles
        .iter()
        .map(|profile| profile.account.id.as_str())
        .collect();
    assert_eq!(ids, ["first", "later"]);
    assert!(profiles.iter().all(|profile| profile.managed));
    assert_eq!(
        profiles
            .iter()
            .map(|profile| profile.created_at_ms)
            .collect::<Vec<_>>(),
        [Some(10), Some(20)]
    );
}

#[test]
fn rejects_metadata_from_the_wrong_provider() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("codex");
    let profile = root.join("wrong");
    fs::create_dir_all(&profile).unwrap();
    write_metadata(
        &profile.join("profile.json"),
        &ProfileMetadata {
            version: PROFILE_VERSION,
            id: "wrong".to_string(),
            provider: Provider::Claude,
            created_at_ms: 1,
        },
    )
    .unwrap();

    assert!(discover_managed_profiles(&root, Provider::Codex).is_empty());
}

#[test]
fn provider_profiles_keep_independent_local_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    let profile = |id: &str, percent: f64| {
        let home = temp.path().join(id);
        let config_dir = home.join(".claude");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            home.join(".claude.json"),
            serde_json::to_vec(&serde_json::json!({
                "oauthAccount": {
                    "emailAddress": format!("{id}@example.com")
                },
                "cachedUsageUtilization": {
                    "fetchedAtMs": 1786956888017i64,
                    "utilization": {
                        "session": {
                            "utilization": percent,
                            "resets_at": "2026-08-17T09:09:59+00:00"
                        }
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        AccountProfile {
            provider: Provider::Claude,
            account: ProviderAccount {
                id: id.to_string(),
                email: None,
            },
            home_dir: home,
            config_dir,
            managed: true,
            created_at_ms: Some(1),
        }
    };

    let first_profile = profile("first", 12.0);
    let second_profile = profile("second", 78.0);
    let first =
        limits::claude::read_from_profile(&first_profile.home_dir, &first_profile.config_dir)
            .unwrap();
    let second =
        limits::claude::read_from_profile(&second_profile.home_dir, &second_profile.config_dir)
            .unwrap();
    assert_eq!(first.account.email.as_deref(), Some("first@example.com"));
    assert_eq!(first.windows[0].percent_used, 12.0);
    assert_eq!(second.account.email.as_deref(), Some("second@example.com"));
    assert_eq!(second.windows[0].percent_used, 78.0);
}

#[test]
fn reads_codex_email_from_the_profile_id_token() {
    let temp = tempfile::tempdir().unwrap();
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(br#"{"email":"codex@example.com"}"#);
    fs::write(
        temp.path().join("auth.json"),
        serde_json::to_vec(&serde_json::json!({
            "tokens": {"id_token": format!("header.{payload}.signature")}
        }))
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        limits::codex::read_email_from_home(temp.path()).as_deref(),
        Some("codex@example.com")
    );
}

#[test]
fn same_email_profiles_keep_distinct_stable_local_identities() {
    let profile = |id: &str| AccountProfile {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: id.into(),
            email: Some("same@example.com".into()),
        },
        home_dir: id.into(),
        config_dir: format!("{id}/.codex").into(),
        managed: true,
        created_at_ms: Some(1),
    };
    let mut profiles = vec![profile("first"), profile("second"), profile("first")];

    retain_unique_profiles(&mut profiles);

    let ids: Vec<_> = profiles
        .iter()
        .map(|profile| profile.account.id.as_str())
        .collect();
    assert_eq!(ids, ["first", "second"]);
}

#[test]
fn reauthentication_resolves_only_the_exact_provider_and_account() {
    let profile = |provider: Provider, id: &str| AccountProfile {
        provider,
        account: ProviderAccount {
            id: id.into(),
            email: Some("same@example.com".into()),
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
    let found = exact_profile(profiles.clone(), Provider::Claude, "second").unwrap();
    assert_eq!(found.account.id, "second");
    assert_eq!(found.provider, Provider::Claude);
    assert!(exact_profile(profiles, Provider::Codex, "missing").is_none());
}
