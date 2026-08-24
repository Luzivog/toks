use super::principal::read_principal_material;

fn profile(account: Option<&str>, email: &str, token: Option<&str>) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join(".claude");
    std::fs::create_dir(&config).unwrap();
    std::fs::write(
        root.path().join(".claude.json"),
        serde_json::json!({"oauthAccount": {
            "accountUuid": account,
            "emailAddress": email
        }})
        .to_string(),
    )
    .unwrap();
    if let Some(token) = token {
        std::fs::write(
            config.join(".credentials.json"),
            serde_json::json!({"claudeAiOauth": {"accessToken": token}}).to_string(),
        )
        .unwrap();
    }
    root
}

#[test]
fn account_uuid_is_stable_across_email_and_token_rotation() {
    let first = profile(Some("account-uuid"), "before@example.com", Some("token-a"));
    let second = profile(Some("account-uuid"), "after@example.com", Some("token-b"));
    assert_eq!(
        read_principal_material(first.path(), &first.path().join(".claude")),
        read_principal_material(second.path(), &second.path().join(".claude"))
    );
}

#[test]
fn email_never_becomes_identity_and_credentials_are_required() {
    let email_only = profile(None, "same@example.com", Some("token"));
    let signed_out = profile(Some("account-uuid"), "same@example.com", None);
    assert!(
        read_principal_material(email_only.path(), &email_only.path().join(".claude")).is_none()
    );
    assert!(
        read_principal_material(signed_out.path(), &signed_out.path().join(".claude")).is_none()
    );
}

#[test]
fn different_account_uuids_do_not_coalesce() {
    let first = profile(Some("account-a"), "same@example.com", Some("token-a"));
    let second = profile(Some("account-b"), "same@example.com", Some("token-b"));
    assert_ne!(
        read_principal_material(first.path(), &first.path().join(".claude")),
        read_principal_material(second.path(), &second.path().join(".claude"))
    );
}
