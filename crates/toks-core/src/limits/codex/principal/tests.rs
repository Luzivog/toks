use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::Value;
use std::path::Path;

use super::{read_principal_material, EXPECTED_ISSUER};

#[test]
fn stable_account_claim_ignores_email_and_token_signature() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for (root, email, signature) in [
        (first.path(), "before@example.com", signature(1)),
        (second.path(), "after@example.com", signature(2)),
    ] {
        write_auth(
            root,
            serde_json::json!({
                "iss": EXPECTED_ISSUER,
                "sub": "user-subject",
                "https://api.openai.com/auth": {"chatgpt_account_id": "account-subject"},
                "email": email
            }),
            "RS256",
            &signature,
        );
    }
    assert_eq!(
        read_principal_material(first.path()),
        read_principal_material(second.path())
    );
}

#[test]
fn rejects_unsigned_wrong_issuer_and_email_only_tokens() {
    let signature = signature(3);
    for (claims, algorithm, signature) in [
        (
            serde_json::json!({"iss": EXPECTED_ISSUER, "sub": "subject"}),
            "none",
            signature.clone(),
        ),
        (
            serde_json::json!({"iss": "https://example.com", "sub": "subject"}),
            "RS256",
            signature.clone(),
        ),
        (
            serde_json::json!({"iss": EXPECTED_ISSUER, "email": "same@example.com"}),
            "RS256",
            signature.clone(),
        ),
        (
            serde_json::json!({"iss": EXPECTED_ISSUER, "sub": "subject"}),
            "RS256",
            signature.clone(),
        ),
        (
            serde_json::json!({"iss": EXPECTED_ISSUER, "sub": "subject"}),
            "RS256",
            String::new(),
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        write_auth(root.path(), claims, algorithm, &signature);
        assert!(read_principal_material(root.path()).is_none());
    }
}

#[test]
fn account_header_requires_a_structurally_valid_matching_account_claim() {
    let account = "account-a";
    let valid = auth_with_claims(
        serde_json::json!({
            "iss": EXPECTED_ISSUER,
            "https://api.openai.com/auth": {"chatgpt_account_id": account}
        }),
        "RS256",
        &signature(4),
    );
    let subject_only = auth_with_claims(
        serde_json::json!({"iss": EXPECTED_ISSUER, "sub": "subject-a"}),
        "RS256",
        &signature(4),
    );
    let unsigned = auth_with_claims(
        serde_json::json!({
            "iss": EXPECTED_ISSUER,
            "https://api.openai.com/auth": {"chatgpt_account_id": account}
        }),
        "none",
        &signature(4),
    );
    let unsupported = auth_with_claims(
        serde_json::json!({
            "iss": EXPECTED_ISSUER,
            "https://api.openai.com/auth": {"chatgpt_account_id": account}
        }),
        "HS256",
        &signature(4),
    );

    assert!(super::account_header_matches_auth(&valid, account));
    assert!(!super::account_header_matches_auth(&valid, "account-b"));
    assert!(!super::account_header_matches_auth(&subject_only, account));
    assert!(!super::account_header_matches_auth(&unsigned, account));
    assert!(!super::account_header_matches_auth(&unsupported, account));
    assert!(!super::account_header_matches_auth(
        &serde_json::json!({"tokens": {"id_token": "malformed"}}),
        account,
    ));
    assert!(!super::account_header_matches_auth(
        &serde_json::json!({"tokens": {}}),
        account,
    ));
}

#[test]
fn rejects_malformed_and_wrong_length_rs256_signatures() {
    let claims = serde_json::json!({
        "iss": EXPECTED_ISSUER,
        "https://api.openai.com/auth": {"chatgpt_account_id": "account-a"}
    });
    for signature in [
        "!".to_string(),
        URL_SAFE_NO_PAD.encode([7_u8; 32]),
        URL_SAFE_NO_PAD.encode([7_u8; 255]),
        URL_SAFE_NO_PAD.encode([7_u8; 257]),
    ] {
        let auth = auth_with_claims(claims.clone(), "RS256", &signature);
        assert!(!super::account_header_matches_auth(&auth, "account-a"));
    }
    let valid_shape = auth_with_claims(claims, "RS256", &URL_SAFE_NO_PAD.encode([7_u8; 256]));
    assert!(super::account_header_matches_auth(
        &valid_shape,
        "account-a"
    ));
}

#[test]
fn different_provider_accounts_do_not_coalesce() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for (root, account) in [(first.path(), "account-a"), (second.path(), "account-b")] {
        write_auth(
            root,
            serde_json::json!({
                "iss": EXPECTED_ISSUER,
                "sub": "shared-user",
                "https://api.openai.com/auth": {"chatgpt_account_id": account}
            }),
            "RS256",
            &signature(5),
        );
    }
    assert_ne!(
        read_principal_material(first.path()),
        read_principal_material(second.path())
    );
}

fn write_auth(root: &Path, claims: Value, algorithm: &str, signature: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        auth_with_claims(claims, algorithm, signature).to_string(),
    )
    .unwrap();
}

fn auth_with_claims(claims: Value, algorithm: &str, signature: &str) -> Value {
    let header = URL_SAFE_NO_PAD.encode(format!(r#"{{"alg":"{algorithm}"}}"#));
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    serde_json::json!({"tokens": {
        "id_token": format!("{header}.{payload}.{signature}")
    }})
}

fn signature(fill: u8) -> String {
    URL_SAFE_NO_PAD.encode([fill; 256])
}
