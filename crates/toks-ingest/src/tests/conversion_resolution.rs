use super::*;
#[test]
fn test_apply_pricing_if_available_prefers_provider_aware_match() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "xai/grok-code-fast-1-0825".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    litellm.insert(
        "azure_ai/grok-code-fast-1".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "opencode",
        "grok-code",
        "azure",
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

    assert_eq!(msg.cost, 0.2);
}

#[test]
fn test_apply_pricing_if_available_uses_nested_reseller_exact_match() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    litellm.insert(
        "azure/openai/gpt-4".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "opencode",
        "gpt-4",
        "azure",
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

    assert_eq!(msg.cost, 0.2);
}

#[test]
fn test_apply_pricing_if_available_keeps_scoped_fireworks_cost_without_exact_pricing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fireworks_ai/accounts/fireworks/models/deepseek-r1-0528-distill-qwen3-8b".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.0000002),
            output_cost_per_token: Some(0.0000002),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "deepseek/deepseek-v4-pro".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.000001),
            output_cost_per_token: Some(0.000002),
            ..Default::default()
        },
    );

    let pricing = pricing::PricingService::new(litellm, openrouter);
    let mut msg = UnifiedMessage::new(
        "opencode",
        "accounts/fireworks/models/deepseek-v4-pro",
        "fireworks",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.123,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.123);
}

#[test]
fn test_apply_pricing_if_available_prefers_provider_specific_exact_match_over_plain_exact() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gemini-2.5-pro".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "google/gemini-2.5-pro".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            cache_creation_input_token_cost: Some(0.01),
            ..Default::default()
        },
    );

    let pricing = pricing::PricingService::new(litellm, openrouter);

    let mut msg = UnifiedMessage::new(
        "opencode",
        "gemini-2.5-pro",
        "google",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 3,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.05);
}

#[test]
fn test_apply_pricing_if_available_normalizes_openai_codex_provider() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-5.2-preview".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    litellm.insert(
        "google/gpt-5.2-preview-max".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.1),
            output_cost_per_token: Some(0.2),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "openclaw",
        "gpt-5.2",
        "openai-codex",
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

    assert_eq!(msg.cost, 0.2);
}
