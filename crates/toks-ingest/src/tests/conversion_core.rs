use super::*;
#[test]
fn test_parsed_round_trip_preserves_workspace_metadata() {
    let mut unified = UnifiedMessage::new(
        "qwen",
        "qwen3.5-plus",
        "qwen",
        "session-1",
        1_742_390_400_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 2,
            cache_write: 0,
            reasoning: 1,
        },
        1.25,
    );
    unified.set_workspace(
        Some("//server/share/demo-workspace".to_string()),
        Some("demo-workspace".to_string()),
    );
    unified.duration_ms = Some(2500);

    let parsed = unified_to_parsed(&unified);
    let round_tripped = parsed_to_unified(&parsed, 2.5);

    assert_eq!(
        round_tripped.workspace_key.as_deref(),
        Some("//server/share/demo-workspace")
    );
    assert_eq!(
        round_tripped.workspace_label.as_deref(),
        Some("demo-workspace")
    );
    assert_eq!(round_tripped.cost, 2.5);
    assert_eq!(round_tripped.duration_ms, Some(2500));
}

#[test]
fn test_apply_pricing_if_available_keeps_existing_cost_without_pricing() {
    let mut msg = UnifiedMessage::new_with_agent(
        "roocode",
        "gpt-4o",
        "provider",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.42,
        Some("planner".to_string()),
    );

    apply_pricing_if_available(&mut msg, None);

    assert_eq!(msg.cost, 0.42);
}

#[test]
fn test_apply_pricing_if_available_overrides_cost_when_pricing_exists() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4o".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "codex",
        "gpt-4o",
        "provider",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.02);
}

#[test]
fn test_apply_pricing_if_available_applies_zed_hosted_markup() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4-5".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "zed",
        "claude-sonnet-4-5",
        crate::sessions::zed::ZED_HOSTED_PROVIDER,
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert!((msg.cost - 0.022).abs() < 1e-12);
}

#[test]
fn test_apply_pricing_if_available_skips_zed_markup_for_non_zed_client() {
    // Non-zed client with provider_id "zed.dev" must not receive the +10%
    // markup. The multiplier is gated on (client == "zed" AND provider).
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4-5".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "claudecode",
        "claude-sonnet-4-5",
        crate::sessions::zed::ZED_HOSTED_PROVIDER,
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    // 10 * 0.001 + 5 * 0.002 = 0.020, no markup.
    assert!((msg.cost - 0.020).abs() < 1e-12);
}

#[test]
fn test_apply_pricing_if_available_skips_zed_markup_for_byok_provider() {
    // A Zed message whose provider_id is the upstream provider directly
    // (BYOK / non-hosted path) must not be marked up — the user is paying
    // the upstream API directly, not through Zed.
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4-5".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "zed",
        "claude-sonnet-4-5",
        "anthropic",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert!((msg.cost - 0.020).abs() < 1e-12);
}

#[test]
fn test_apply_pricing_if_available_uses_reasoning_for_gemini() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gemini-2.5-pro".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "gemini",
        "gemini-2.5-pro",
        "google",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 7,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.034);
}

#[test]
fn test_apply_pricing_if_available_uses_cache_read_pricing_for_gemini() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gemini-2.5-pro".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            cache_read_input_token_cost: Some(0.0001),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "gemini",
        "gemini-2.5-pro",
        "google",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 7,
            cache_write: 0,
            reasoning: 3,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.0267);
}

#[test]
fn test_apply_pricing_if_available_uses_market_rate_for_free_variant() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "z-ai/glm-4.7".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(HashMap::new(), openrouter);

    let mut msg = UnifiedMessage::new(
        "opencode",
        "glm-4.7-free",
        "modal",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.02);
}
