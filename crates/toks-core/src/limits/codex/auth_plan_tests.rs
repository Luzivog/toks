use super::auth_plan::read_plan_from_auth;
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
