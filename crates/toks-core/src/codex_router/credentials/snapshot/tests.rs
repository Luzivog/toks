use std::fs;

use crate::accounts::{
    AccountId, AccountIdentityKind, AccountProfile, CredentialProfileId, ProviderAccount,
};

use super::{credential, snapshot_with};

#[test]
fn replacement_between_profile_discovery_and_auth_read_cannot_mislabel_the_new_token() {
    let directory = tempfile::tempdir().unwrap();
    let profile_id = CredentialProfileId::new("race");
    let profile = profile(directory.path(), profile_id.clone());
    write_auth(directory.path(), "account-a", "token-a");
    let discovered_account =
        production_identity(&profile_id, &read_value(directory.path())).unwrap();

    write_auth(directory.path(), "account-b", "token-b");
    let replacement = snapshot_with(&profile, production_identity).unwrap();
    let error = match credential(
        replacement.profile_id,
        &discovered_account,
        replacement.auth,
    ) {
        Ok(_) => panic!("replacement identity inherited the stale account label"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        crate::codex_router::credentials::CredentialError::NeedsSignIn(_)
    ));
}

#[test]
fn unverifiable_id_tokens_never_become_routable_profile_fallback_accounts() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let directory = tempfile::tempdir().unwrap();
    let profile = profile(directory.path(), CredentialProfileId::new("unverifiable"));
    let unsigned_header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let signed_header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
    let account_claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "iss": "https://auth.openai.com",
            "https://api.openai.com/auth": {"chatgpt_account_id": "account-a"}
        })
        .to_string(),
    );
    let subject_claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({"iss": "https://auth.openai.com", "sub": "subject-a"}).to_string(),
    );
    let signature = URL_SAFE_NO_PAD.encode([7_u8; 256]);
    for id_token in [
        None,
        Some("malformed".to_string()),
        Some(format!("{unsigned_header}.{account_claims}.{signature}")),
        Some(format!("{signed_header}.{subject_claims}.{signature}")),
    ] {
        let mut auth = serde_json::json!({"tokens": {
            "access_token": "access",
            "refresh_token": "refresh",
            "account_id": "account-a"
        }});
        if let Some(id_token) = id_token {
            auth["tokens"]["id_token"] = serde_json::Value::String(id_token);
        }
        fs::write(
            directory.path().join("auth.json"),
            serde_json::to_vec(&auth).unwrap(),
        )
        .unwrap();

        assert!(crate::accounts::read_codex_auth_for_test(&profile).is_err());
    }
}

#[test]
fn signed_identity_and_outgoing_account_header_must_come_from_the_same_account() {
    let directory = tempfile::tempdir().unwrap();
    write_auth(directory.path(), "account-a", "token-a");
    let path = directory.path().join("auth.json");
    let mut auth = read_value(directory.path());
    auth["tokens"]["account_id"] = serde_json::Value::String("account-b".into());
    fs::write(&path, serde_json::to_vec(&auth).unwrap()).unwrap();

    assert!(!crate::limits::codex::account_header_matches_auth(
        &auth,
        "account-b"
    ));
}

fn production_identity(
    profile: &CredentialProfileId,
    auth: &serde_json::Value,
) -> Option<AccountId> {
    crate::accounts::codex_auth_account_id_for_test(profile, auth)
}

fn read_value(directory: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(directory.join("auth.json")).unwrap()).unwrap()
}

fn write_auth(directory: &std::path::Path, account: &str, token: &str) {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let replacement = directory.join("auth.next");
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "iss":"https://auth.openai.com",
            "https://api.openai.com/auth":{"chatgpt_account_id":account}
        })
        .to_string(),
    );
    let signature = URL_SAFE_NO_PAD.encode([7_u8; 256]);
    let id_token = format!("{header}.{claims}.{signature}");
    fs::write(
        &replacement,
        serde_json::json!({
            "tokens": {
                "access_token": token,
                "refresh_token": "refresh",
                "account_id": account,
                "id_token": id_token
            }
        })
        .to_string(),
    )
    .unwrap();
    fs::rename(replacement, directory.join("auth.json")).unwrap();
}

fn profile(directory: &std::path::Path, profile_id: CredentialProfileId) -> AccountProfile {
    AccountProfile {
        provider: crate::limits::Provider::Codex,
        profile_id,
        account: ProviderAccount {
            id: AccountId::new("account-a"),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: None,
            sources: Vec::new(),
        },
        home_dir: directory.into(),
        config_dir: directory.into(),
        managed: true,
        created_at_ms: Some(1),
    }
}
