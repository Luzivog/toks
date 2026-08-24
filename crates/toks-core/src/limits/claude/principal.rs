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
