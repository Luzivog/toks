use super::super::*;

#[test]
fn test_provider_prefixed_non_opus_prefers_exact_openrouter_without_tier_advantage() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000123),
            output_cost_per_token: Some(0.0000456),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-sonnet-4").unwrap();
    assert_eq!(resolved.source, "OpenRouter");
    assert_eq!(resolved.matched_key, "anthropic/claude-sonnet-4");
}

#[test]
fn test_provider_prefixed_exact_litellm_beats_stripped_generic_match() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            ..Default::default()
        },
    );
    litellm.insert(
        "openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let resolved = lookup.lookup("openai/gpt-4").unwrap();
    assert_eq!(resolved.source, "LiteLLM");
    assert_eq!(resolved.matched_key, "openai/gpt-4");
}

#[test]
fn test_provider_prefixed_override_requires_valid_base_and_above_pair() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4".into(),
        ModelPricing {
            // Above tier exists, but corresponding base is missing.
            // This must not qualify for provider-prefixed override.
            input_cost_per_token: None,
            input_cost_per_token_above_200k_tokens: Some(0.00002),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000123),
            output_cost_per_token: Some(0.0000456),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-sonnet-4").unwrap();
    assert_eq!(resolved.source, "OpenRouter");
    assert_eq!(resolved.matched_key, "anthropic/claude-sonnet-4");
}

#[test]
fn test_provider_prefixed_override_rejects_invalid_base_even_with_above() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(f64::NAN),
            input_cost_per_token_above_200k_tokens: Some(0.00002),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000123),
            output_cost_per_token: Some(0.0000456),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-sonnet-4").unwrap();
    assert_eq!(resolved.source, "OpenRouter");
    assert_eq!(resolved.matched_key, "anthropic/claude-sonnet-4");
}

#[test]
fn test_provider_prefixed_override_allows_zero_base_with_valid_above() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4".into(),
        ModelPricing {
            // Policy: base=0 with valid above is a valid tier pair.
            input_cost_per_token: Some(0.0),
            input_cost_per_token_above_200k_tokens: Some(0.00002),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000123),
            output_cost_per_token: Some(0.0000456),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-sonnet-4").unwrap();
    assert_eq!(resolved.source, "LiteLLM");
    assert_eq!(resolved.matched_key, "claude-sonnet-4");
}

#[test]
fn test_provider_prefixed_cache_only_tier_keeps_exact_openrouter() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4".into(),
        ModelPricing {
            cache_read_input_token_cost: Some(0.0000001),
            cache_read_input_token_cost_above_200k_tokens: Some(0.0000002),
            cache_creation_input_token_cost: Some(0.0000003),
            cache_creation_input_token_cost_above_200k_tokens: Some(0.0000004),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000123),
            output_cost_per_token: Some(0.0000456),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-sonnet-4").unwrap();
    assert_eq!(resolved.source, "OpenRouter");
    assert_eq!(resolved.matched_key, "anthropic/claude-sonnet-4");
}

#[test]
fn test_provider_prefixed_opus_4_6_prefers_litellm_tiered_pricing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            input_cost_per_token_above_200k_tokens: Some(0.00002),
            output_cost_per_token: Some(0.00005),
            output_cost_per_token_above_200k_tokens: Some(0.00006),
            cache_read_input_token_cost: Some(0.000001),
            cache_read_input_token_cost_above_200k_tokens: Some(0.000002),
            cache_creation_input_token_cost: Some(0.000003),
            cache_creation_input_token_cost_above_200k_tokens: Some(0.000004),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.123),
            output_cost_per_token: Some(0.456),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-opus-4-6").unwrap();
    assert_eq!(resolved.source, "LiteLLM");
    assert_eq!(resolved.matched_key, "claude-opus-4-6");

    let cost = lookup.calculate_cost("anthropic/claude-opus-4-6", 200_001, 0, 0, 0, 0);
    let expected = 200_000.0 * 0.00001 + 0.00002;
    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_anthropic_prefixed_sonnet_variant_uses_canonical_pricing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-sonnet-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_read_input_token_cost: Some(0.0000003),
            cache_creation_input_token_cost: Some(0.00000375),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-4-6-sonnet").unwrap();
    assert_eq!(resolved.source, "LiteLLM");
    assert_eq!(resolved.matched_key, "claude-sonnet-4-6");

    let cost = lookup.calculate_cost("anthropic/claude-4-6-sonnet", 100, 20, 10, 5, 0);
    let expected = 100.0 * 0.000003 + 20.0 * 0.000015 + 10.0 * 0.0000003 + 5.0 * 0.00000375;
    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_anthropic_prefixed_haiku_variant_uses_canonical_pricing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-haiku-4-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000008),
            output_cost_per_token: Some(0.000004),
            cache_read_input_token_cost: Some(0.00000008),
            cache_creation_input_token_cost: Some(0.000001),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let resolved = lookup.lookup("anthropic/claude-4-5-haiku").unwrap();
    assert_eq!(resolved.source, "LiteLLM");
    assert_eq!(resolved.matched_key, "claude-haiku-4-5");

    let cost = lookup.calculate_cost("anthropic/claude-4-5-haiku", 100, 20, 10, 5, 0);
    let expected = 100.0 * 0.0000008 + 20.0 * 0.000004 + 10.0 * 0.00000008 + 5.0 * 0.000001;
    assert!((cost - expected).abs() < 1e-12);
}
