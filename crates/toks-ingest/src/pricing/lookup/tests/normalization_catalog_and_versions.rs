use super::super::*;
use super::{create_lookup, mock_litellm, mock_openrouter};

#[test]
fn test_opencode_zen_claude_3_5_haiku() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-3-5-haiku").unwrap();
    assert_eq!(result.matched_key, "anthropic/claude-3.5-haiku");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_glm_4_7_with_hyphen() {
    let lookup = create_lookup();
    let result = lookup.lookup("glm-4-7").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.7");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_glm_4_6_with_hyphen() {
    let lookup = create_lookup();
    let result = lookup.lookup("glm-4-6").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.6");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_big_pickle() {
    let lookup = create_lookup();
    let result = lookup.lookup("big-pickle").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.7");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn antigravity_model_aliases_reach_priced_catalog_entries() {
    let mut litellm = mock_litellm();
    litellm.insert(
        "gemini-3.1-pro".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000002),
            output_cost_per_token: Some(0.000012),
            ..Default::default()
        },
    );
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "google/gemini-3.5-flash".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000015),
            output_cost_per_token: Some(0.000009),
            cache_read_input_token_cost: Some(0.00000015),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new_with_models_dev(
        litellm,
        mock_openrouter(),
        HashMap::new(),
        HashMap::new(),
        models_dev,
    );

    let cases = [
        ("MODEL_PLACEHOLDER_M16", "gemini-3.1-pro", "LiteLLM"),
        (
            "MODEL_PLACEHOLDER_M84",
            "vertex_ai/gemini-3-flash-preview",
            "LiteLLM",
        ),
        (
            "MODEL_PLACEHOLDER_M133",
            "google/gemini-3.5-flash",
            "Models.dev",
        ),
        (
            "gemini-3-flash-agent",
            "google/gemini-3.5-flash",
            "Models.dev",
        ),
        ("gemini-3-flash-b", "google/gemini-3.5-flash", "Models.dev"),
        (
            // Legacy CLI responseModel for M132, the retired predecessor
            // of M133 — prices as the High tier, same catalog entry as
            // `gemini-3-flash-agent`/`gemini-3-flash-b` above (see
            // aliases.rs source-citation comment, models.ts@603e3ea).
            "gemini-3-flash-a",
            "google/gemini-3.5-flash",
            "Models.dev",
        ),
        (
            "MODEL_PLACEHOLDER_M187",
            "google/gemini-3.5-flash",
            "Models.dev",
        ),
        (
            "MODEL_PLACEHOLDER_M20",
            "google/gemini-3.5-flash",
            "Models.dev",
        ),
    ];

    for (raw, expected_key, expected_source) in cases {
        let result = lookup
            .lookup(raw)
            .unwrap_or_else(|| panic!("unpriced alias: {raw}"));
        assert_eq!(result.matched_key, expected_key, "raw model: {raw}");
        assert_eq!(result.source, expected_source, "raw model: {raw}");
    }

    let cost = lookup.calculate_cost("gemini-3-flash-agent", 1_000_000, 100_000, 50_000, 0, 0);
    assert!((cost - 2.4075).abs() < 1e-10);
}

#[test]
fn test_opencode_zen_kimi_k2_6_aliases() {
    let lookup = create_lookup();
    for model_id in ["k2p6", "k2-p6", "kimi-k2p6", "Kimi-K2.6"] {
        let result = lookup.lookup(model_id).unwrap();
        assert_eq!(result.matched_key, "moonshotai/kimi-k2.6");
        assert_eq!(result.source, "OpenRouter");
        assert_eq!(result.pricing.input_cost_per_token, Some(9.5e-7));
        assert_eq!(result.pricing.output_cost_per_token, Some(0.000004));
    }
}

#[test]
fn test_opencode_zen_kimi_k2_5_aliases_unchanged() {
    let lookup = create_lookup();

    let raw_k2p5 = lookup.lookup("k2p5").unwrap();
    assert_eq!(raw_k2p5.matched_key, "moonshotai/kimi-k2-thinking");

    let dotted = lookup.lookup("kimi-k2.5").unwrap();
    assert_eq!(dotted.matched_key, "moonshotai/kimi-k2.5");
}

#[test]
fn test_normalize_opus_4_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("opus-4-5").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_free_variant_normalizes_to_market_priced_claude_model() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4-5-free").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_free_variant_with_extra_suffix_falls_back_to_market_priced_model() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4-5-free-high").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_normalize_opus_4_6_prefers_4_6_over_4() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00002),
            output_cost_per_token: Some(0.0001),
            ..Default::default()
        },
    );
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00005),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("opus-4-6").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-6");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_normalize_opus_4_6_dot_prefers_4_6_over_4() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00002),
            output_cost_per_token: Some(0.0001),
            ..Default::default()
        },
    );
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00005),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("opus-4.6").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-6");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_normalize_opus_4_60_does_not_degrade_to_opus_4() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00002),
            output_cost_per_token: Some(0.0001),
            ..Default::default()
        },
    );
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00005),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    assert!(lookup.lookup("opus-4-60").is_none());
}

#[test]
fn test_normalize_opus_4_7_prefers_4_7_over_4() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            ..Default::default()
        },
    );
    litellm.insert(
        "claude-opus-4-7".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("opus-4-7").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-7");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_normalize_opus_4_7_dot_prefers_4_7_over_4() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            ..Default::default()
        },
    );
    litellm.insert(
        "claude-opus-4-7".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("opus-4.7").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-7");
    assert_eq!(result.source, "LiteLLM");
}

/// Regression: `aws.claude-opus-4-7` (Bedrock-style id) used to degrade
/// to OpenRouter's `anthropic/claude-opus-4` ($15/$75/$1.50/$18.75 per M)
/// because `normalize_model_name` only knew 4.5/4.6 and fell through to
/// the bare `claude-opus-4` branch — which OpenRouter then resolved via
/// `model_part` index to the legacy opus 4 entry. Result was ~3x overcharge.
#[test]
fn test_aws_opus_4_7_does_not_degrade_to_opus_4() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4-7".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            cache_read_input_token_cost: Some(5e-7),
            cache_creation_input_token_cost: Some(0.00000625),
            ..Default::default()
        },
    );
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_read_input_token_cost: Some(0.0000015),
            cache_creation_input_token_cost: Some(0.00001875),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let result = lookup.lookup("aws.claude-opus-4-7").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-7");
    assert_ne!(result.matched_key, "anthropic/claude-opus-4");

    // 8.4M input + 873K output + 41.3M cache_read + 12.1M cache_write
    // at opus-4-7 rates should be ~$160, not ~$480 (legacy opus 4).
    let cost = lookup.calculate_cost(
        "aws.claude-opus-4-7",
        8_400_000,
        873_000,
        41_300_000,
        12_100_000,
        0,
    );
    assert!(
        (140.0..=180.0).contains(&cost),
        "expected opus-4-7 priced cost around $160, got ${cost:.2}"
    );
}

#[test]
fn test_unknown_future_opus_minor_does_not_degrade_to_opus_4() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(HashMap::new(), openrouter, HashMap::new());

    assert!(lookup.lookup("claude-opus-4-8").is_none());
    assert!(lookup.lookup("aws.claude-opus-4-8").is_none());
}

#[test]
fn test_normalize_opus_14_6_does_not_map_to_4_6() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00005),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    assert!(lookup.lookup("opus-14-6").is_none());
}

#[test]
fn test_normalize_sonnet_14_5_does_not_map_to_4_5() {
    assert_eq!(normalize_model_name("sonnet-14-5"), None);
}

#[test]
fn test_normalize_haiku_14_5_does_not_map_to_4_5() {
    assert_eq!(normalize_model_name("haiku-14-5"), None);
}
