use super::*;

#[test]
fn test_is_synthetic_model_hf_prefix() {
    assert!(is_synthetic_model("hf:deepseek-ai/DeepSeek-V3-0324"));
    assert!(is_synthetic_model("hf:zai-org/GLM-4.7"));
    assert!(is_synthetic_model("hf:moonshotai/Kimi-K2.5"));
    assert!(is_synthetic_model("hf:MiniMaxAI/MiniMax-M2.1"));
}

#[test]
fn test_is_synthetic_model_fireworks_prefix() {
    assert!(is_synthetic_model(
        "accounts/fireworks/models/deepseek-v3-0324"
    ));
    assert!(is_synthetic_model("accounts/fireworks/models/glm-4.7"));
}

#[test]
fn test_is_synthetic_model_together_prefix() {
    assert!(is_synthetic_model("accounts/together/models/qwen3-235b"));
}

#[test]
fn test_is_synthetic_model_negative() {
    assert!(!is_synthetic_model("claude-sonnet-4-5"));
    assert!(!is_synthetic_model("gpt-5.2-codex"));
    assert!(!is_synthetic_model("deepseek-v3"));
    assert!(!is_synthetic_model("gemini-2.5-pro"));
}

#[test]
fn test_is_synthetic_provider() {
    assert!(is_synthetic_provider("synthetic"));
    assert!(is_synthetic_provider("glhf"));
    assert!(is_synthetic_provider("Synthetic"));
    assert!(is_synthetic_provider("GLHF"));
    assert!(is_synthetic_provider("synthetic.new"));
    assert!(is_synthetic_provider("octofriend"));
}

#[test]
fn test_is_synthetic_provider_negative() {
    assert!(!is_synthetic_provider("anthropic"));
    assert!(!is_synthetic_provider("openai"));
    assert!(!is_synthetic_provider("moonshot"));
    assert!(!is_synthetic_provider("fireworks"));
}

#[test]
fn test_normalize_synthetic_model_hf() {
    assert_eq!(
        normalize_synthetic_model("hf:deepseek-ai/DeepSeek-V3-0324"),
        "deepseek-v3-0324"
    );
    assert_eq!(normalize_synthetic_model("hf:zai-org/GLM-4.7"), "glm-4.7");
    assert_eq!(
        normalize_synthetic_model("hf:moonshotai/Kimi-K2.5"),
        "kimi-k2.5"
    );
}

#[test]
fn test_normalize_synthetic_model_fireworks() {
    assert_eq!(
        normalize_synthetic_model("accounts/fireworks/models/deepseek-v3-0324"),
        "deepseek-v3-0324"
    );
}

#[test]
fn test_normalize_synthetic_model_passthrough() {
    assert_eq!(
        normalize_synthetic_model("claude-sonnet-4-5"),
        "claude-sonnet-4-5"
    );
    assert_eq!(normalize_synthetic_model("gpt-4o"), "gpt-4o");
}

#[test]
fn test_normalize_synthetic_gateway_fields_sets_provider_when_unknown() {
    let mut model_id = "hf:deepseek-ai/DeepSeek-V3-0324".to_string();
    let mut provider_id = "unknown".to_string();

    let matched = normalize_synthetic_gateway_fields(&mut model_id, &mut provider_id);

    assert!(matched);
    assert_eq!(model_id, "deepseek-v3-0324");
    assert_eq!(provider_id, "synthetic");
}

#[test]
fn test_normalize_synthetic_gateway_fields_preserves_existing_provider() {
    let mut model_id = "accounts/fireworks/models/deepseek-v3-0324".to_string();
    let mut provider_id = "fireworks".to_string();

    let matched = normalize_synthetic_gateway_fields(&mut model_id, &mut provider_id);

    assert!(matched);
    assert_eq!(model_id, "deepseek-v3-0324");
    assert_eq!(provider_id, "fireworks");
}

#[test]
fn test_matches_synthetic_filter_accepts_gateway_traffic_without_rewriting_client() {
    assert!(matches_synthetic_filter(
        "opencode",
        "hf:deepseek-ai/DeepSeek-V3-0324",
        "unknown"
    ));
    assert!(matches_synthetic_filter(
        "claude",
        "claude-sonnet-4-5",
        "glhf"
    ));
    assert!(!matches_synthetic_filter("opencode", "gpt-4o", "anthropic"));
}

#[test]
fn test_parse_octofriend_sqlite_nonexistent() {
    let result = parse_octofriend_sqlite(Path::new("/nonexistent/path/sqlite.db"));
    assert!(result.is_empty());
}
