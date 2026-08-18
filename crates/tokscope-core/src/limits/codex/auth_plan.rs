use std::path::Path;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;

use crate::limits::{PlanDetails, PlanMultiplier};

const OPENAI_AUTH_CLAIM: &str = "/https:~1~1api.openai.com~1auth/chatgpt_plan_type";

/// Read the raw ChatGPT product SKU from Codex's signed token payload.
/// Tokens are decoded in memory only and are never retained or logged.
pub(crate) fn read_plan_from_auth(config_dir: &Path) -> PlanDetails {
    let Some(auth) = std::fs::read_to_string(config_dir.join("auth.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return PlanDetails::default();
    };
    ["id_token", "access_token"]
        .iter()
        .find_map(|token_name| {
            auth.pointer(&format!("/tokens/{token_name}"))
                .and_then(Value::as_str)
                .and_then(plan_from_token)
        })
        .unwrap_or_default()
}

fn plan_from_token(token: &str) -> Option<PlanDetails> {
    let payload = token.split('.').nth(1)?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
    let name = claims.pointer(OPENAI_AUTH_CLAIM)?.as_str()?.to_string();
    Some(PlanDetails {
        multiplier: PlanMultiplier::from_codex_plan_type(&name),
        name: Some(name),
    })
}

#[cfg(test)]
mod tests {
    use super::read_plan_from_auth;
    use crate::limits::PlanMultiplier;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    #[test]
    fn reads_codex_prolite_from_token_claims() {
        let directory = tempfile::tempdir().unwrap();
        let claims = serde_json::json!({
            "https://api.openai.com/auth": {"chatgpt_plan_type": "prolite"}
        });
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        std::fs::write(
            directory.path().join("auth.json"),
            serde_json::json!({"tokens": {"id_token": format!("e30.{payload}.sig")}}).to_string(),
        )
        .unwrap();
        let plan = read_plan_from_auth(directory.path());
        assert_eq!(plan.name.as_deref(), Some("prolite"));
        assert_eq!(plan.multiplier, Some(PlanMultiplier::Five));
    }
}
