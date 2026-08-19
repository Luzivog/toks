use std::path::Path;

use serde_json::Value;

const PROVIDER_SCOPE: &str = "claude";

/// Return provider-scoped identity material for an authenticated Claude profile.
/// Claude's access token is opaque, so its official account UUID cache supplies
/// the stable subject while credentials prove that the exact profile is signed in.
pub(crate) fn read_principal_material(home: &Path, config_dir: &Path) -> Option<Vec<u8>> {
    let credentials: Value =
        serde_json::from_slice(&std::fs::read(config_dir.join(".credentials.json")).ok()?).ok()?;
    let oauth = credentials.get("claudeAiOauth")?.as_object()?;
    let has_credential = ["accessToken", "refreshToken"].iter().any(|key| {
        oauth
            .get(*key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    });
    if !has_credential {
        return None;
    }

    let root: Value =
        serde_json::from_slice(&std::fs::read(super::claude_json_path(home, config_dir)).ok()?)
            .ok()?;
    let subject = root
        .pointer("/oauthAccount/accountUuid")?
        .as_str()
        .filter(|value| !value.trim().is_empty())?;
    Some(frame(&[PROVIDER_SCOPE, "account", subject]))
}

fn frame(components: &[&str]) -> Vec<u8> {
    let mut result = Vec::new();
    for component in components {
        let bytes = component.as_bytes();
        let length = u32::try_from(bytes.len()).expect("principal component is bounded");
        result.extend_from_slice(&length.to_be_bytes());
        result.extend_from_slice(bytes);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::read_principal_material;

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
            read_principal_material(email_only.path(), &email_only.path().join(".claude"))
                .is_none()
        );
        assert!(
            read_principal_material(signed_out.path(), &signed_out.path().join(".claude"))
                .is_none()
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
}
