use super::plan::{read_claude_plan, PlanMultiplier};

#[test]
fn parses_only_explicit_supported_multipliers() {
    let tier = serde_json::json!({"rateLimitTier": "default_claude_max_20x"});
    assert_eq!(
        PlanMultiplier::from_explicit_metadata(&tier),
        Some(PlanMultiplier::Twenty)
    );
    let numeric = serde_json::json!({"usage_multiplier": 5});
    assert_eq!(
        PlanMultiplier::from_explicit_metadata(&numeric),
        Some(PlanMultiplier::Five)
    );
    let plan_only = serde_json::json!({"plan_type": "max_20x"});
    assert_eq!(PlanMultiplier::from_explicit_metadata(&plan_only), None);
}

#[test]
fn codex_product_skus_map_without_guessing_unknown_plans() {
    assert_eq!(
        PlanMultiplier::from_codex_plan_type("prolite"),
        Some(PlanMultiplier::Five)
    );
    assert_eq!(
        PlanMultiplier::from_codex_plan_type("pro"),
        Some(PlanMultiplier::Twenty)
    );
    assert_eq!(PlanMultiplier::from_codex_plan_type("plus"), None);
    assert_eq!(PlanMultiplier::from_codex_plan_type("future_pro"), None);
}

#[test]
fn reads_claudes_plan_and_explicit_tier_without_tokens() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join(".credentials.json"),
        serde_json::json!({
            "claudeAiOauth": {
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_5x",
                "accessToken": "not-read"
            }
        })
        .to_string(),
    )
    .unwrap();
    let details = read_claude_plan(directory.path());
    assert_eq!(details.name.as_deref(), Some("max"));
    assert_eq!(details.multiplier, Some(PlanMultiplier::Five));
}
