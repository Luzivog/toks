use super::super::cost::{
    compute_cost_for_lookup, has_complete_openai_272k_pricing, should_prefer_openai_tiered_litellm,
    uses_openai_full_request_272k_pricing,
};
use super::super::*;
use super::create_lookup;

#[test]
fn incomplete_unhinted_result_does_not_replace_provider_pricing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "azure/gpt-fallback-guard".into(),
        ModelPricing {
            input_cost_per_token: Some(1.0),
            ..Default::default()
        },
    );
    litellm.insert(
        "gpt-fallback-guard".into(),
        ModelPricing {
            output_cost_per_token: Some(2.0),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let usage = TokenBreakdown {
        input: 1,
        output: 1,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };

    // Neither row covers both populated buckets, and they share no base
    // bucket that would show they price the same deal, so no rate is
    // borrowed. Retain the provider row rather than replacing it with an
    // unhinted row that silently prices the input bucket at zero.
    assert_eq!(
        lookup.calculate_cost_with_provider("gpt-fallback-guard", Some("azure"), &usage),
        1.0
    );
}

// =========================================================================
// COST CALCULATION TESTS
// =========================================================================

#[test]
fn test_calculate_cost_gpt_5_2() {
    let lookup = create_lookup();
    // 1M input, 500K output tokens
    let cost = lookup.calculate_cost("gpt-5.2", 1_000_000, 500_000, 0, 0, 0);
    // input: 1M * 0.00000175 = 1.75, output: 500K * 0.000014 = 7.0
    assert!((cost - 8.75).abs() < 0.001);
}

#[test]
fn test_calculate_cost_claude_sonnet_4_5() {
    let lookup = create_lookup();
    // 100K input, 50K output, 200K cache read
    let cost = lookup.calculate_cost("claude-sonnet-4-5", 100_000, 50_000, 200_000, 0, 0);
    // input: 100K * 0.000003 = 0.30, output: 50K * 0.000015 = 0.75, cache: 200K * 3e-7 = 0.06
    assert!((cost - 1.11).abs() < 0.001);
}

#[test]
fn test_compute_cost_tiered_boundary_at_200k_uses_base_rates() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.000001,
            "input_cost_per_token_above_200k_tokens": 0.000002,
            "output_cost_per_token": 0.000003,
            "output_cost_per_token_above_200k_tokens": 0.000004
        }"#,
    )
    .unwrap();

    let cost = compute_cost(&pricing, 200_000, 200_000, 0, 0, 0);
    let expected = 200_000.0 * 0.000001 + 200_000.0 * 0.000003;

    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_above_200k_splits_input_and_output() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.000001,
            "input_cost_per_token_above_200k_tokens": 0.000002,
            "output_cost_per_token": 0.000003,
            "output_cost_per_token_above_200k_tokens": 0.000004
        }"#,
    )
    .unwrap();

    let cost = compute_cost(&pricing, 200_001, 200_001, 0, 0, 0);
    let expected =
        (200_000.0 * 0.000001 + 1.0 * 0.000002) + (200_000.0 * 0.000003 + 1.0 * 0.000004);

    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_above_272k_splits_gpt_5_5_tokens() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.000005,
            "input_cost_per_token_above_272k_tokens": 0.000010,
            "output_cost_per_token": 0.000030,
            "output_cost_per_token_above_272k_tokens": 0.000045,
            "cache_read_input_token_cost": 0.0000005,
            "cache_read_input_token_cost_above_272k_tokens": 0.000001
        }"#,
    )
    .unwrap();

    let cost = compute_cost(&pricing, 272_001, 272_001, 272_001, 0, 0);
    let expected = (272_000.0 * 0.000005 + 1.0 * 0.000010)
        + (272_000.0 * 0.000030 + 1.0 * 0.000045)
        + (272_000.0 * 0.0000005 + 1.0 * 0.000001);

    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_uses_multiple_thresholds_in_order() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.000001,
            "input_cost_per_token_above_128k_tokens": 0.000002,
            "input_cost_per_token_above_256k_tokens": 0.000003,
            "input_cost_per_token_above_272k_tokens": 0.000004
        }"#,
    )
    .unwrap();

    let cost = compute_cost(&pricing, 300_000, 0, 0, 0, 0);
    let expected = (128_000.0 * 0.000001)
        + (128_000.0 * 0.000002)
        + (16_000.0 * 0.000003)
        + (28_000.0 * 0.000004);

    assert!((cost - expected).abs() < 1e-12);
}

fn openai_272k_result(key: &str, source: &str) -> LookupResult {
    LookupResult {
        matched_key: key.into(),
        source: source.into(),
        pricing: ModelPricing {
            input_cost_per_token: Some(0.000005),
            input_cost_per_token_above_272k_tokens: Some(0.000010),
            output_cost_per_token: Some(0.000030),
            output_cost_per_token_above_272k_tokens: Some(0.000045),
            cache_read_input_token_cost: Some(0.0000005),
            cache_read_input_token_cost_above_272k_tokens: Some(0.000001),
            cache_creation_input_token_cost: Some(0.00000625),
            ..Default::default()
        },
    }
}

#[test]
fn test_openai_272k_full_request_pricing_uses_combined_input() {
    let result = openai_272k_result("openai/gpt-5.5", "LiteLLM");
    let usage = |input, output, cache_read, cache_write| TokenBreakdown {
        input,
        output,
        cache_read,
        cache_write,
        reasoning: 0,
    };
    let cost = compute_cost_for_lookup(&result, Some("openai"), &usage(200_000, 10_000, 72_000, 1));
    let expected = 200_000.0 * 0.000010 + 10_000.0 * 0.000045 + 72_000.0 * 0.000001 + 0.0000125;
    assert!((cost - expected).abs() < 1e-12);

    let boundary = compute_cost_for_lookup(&result, None, &usage(200_000, 10_000, 72_000, 0));
    let boundary_expected = 200_000.0 * 0.000005 + 10_000.0 * 0.000030 + 72_000.0 * 0.0000005;
    assert!((boundary - boundary_expected).abs() < 1e-12);

    let output_only = compute_cost_for_lookup(&result, None, &usage(1, 300_000, 0, 0));
    assert!((output_only - (0.000005 + 300_000.0 * 0.000030)).abs() < 1e-12);
}

#[test]
fn test_provider_aware_openai_prefers_complete_litellm_tiers() {
    let litellm_pricing = openai_272k_result("gpt-5.6-sol", "LiteLLM").pricing;
    let openrouter_pricing = ModelPricing {
        input_cost_per_token: litellm_pricing.input_cost_per_token,
        output_cost_per_token: litellm_pricing.output_cost_per_token,
        cache_read_input_token_cost: litellm_pricing.cache_read_input_token_cost,
        ..Default::default()
    };
    let lookup = PricingLookup::new(
        HashMap::from([("gpt-5.6-sol".into(), litellm_pricing.clone())]),
        HashMap::from([("openai/gpt-5.6-sol".into(), openrouter_pricing)]),
        HashMap::new(),
    );

    let result = lookup
        .lookup_with_provider("gpt-5.6-sol", Some("openai"))
        .unwrap();
    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.matched_key, "gpt-5.6-sol");

    let usage = TokenBreakdown {
        input: 200_000,
        output: 10_000,
        cache_read: 72_001,
        ..Default::default()
    };
    let expected = 200_000.0 * 0.000010 + 10_000.0 * 0.000045 + 72_001.0 * 0.000001;
    for provider in [Some("openai"), Some("unknown"), Some(""), None] {
        let cost = lookup.calculate_cost_with_provider("gpt-5.6-sol", provider, &usage);
        assert!((cost - expected).abs() < 1e-12);
    }

    let lookup = PricingLookup::new(
        HashMap::from([("gpt-5.6-sol".into(), litellm_pricing.clone())]),
        HashMap::from([("openai/gpt-5.6-sol".into(), litellm_pricing)]),
        HashMap::new(),
    );
    let result = lookup
        .lookup_with_provider("gpt-5.6-sol", Some("openai"))
        .unwrap();
    assert_eq!(result.source, "LiteLLM");
    assert!(!should_prefer_openai_tiered_litellm(
        "gpt-5.6-sol",
        Some("openrouter"),
        Some(&result)
    ));
}

#[test]
fn test_openai_tiered_litellm_preference_requires_complete_272k_pricing() {
    let pricing = openai_272k_result("gpt-5.6-sol", "LiteLLM").pricing;
    assert!(has_complete_openai_272k_pricing(&pricing));

    let clear_required: [fn(&mut ModelPricing); 5] = [
        |pricing| pricing.input_cost_per_token = None,
        |pricing| pricing.input_cost_per_token_above_272k_tokens = None,
        |pricing| pricing.output_cost_per_token = None,
        |pricing| pricing.output_cost_per_token_above_272k_tokens = None,
        |pricing| pricing.cache_read_input_token_cost_above_272k_tokens = None,
    ];
    for clear in clear_required {
        let mut incomplete = pricing.clone();
        clear(&mut incomplete);
        assert!(!has_complete_openai_272k_pricing(&incomplete));
    }

    // A fully-absent cache_read pair is now incomplete too: this used to
    // pass leniently, letting the 272k preference silently drop an
    // OpenRouter entry's cache-read pricing (see
    // openai_272k_preference_prefers_openrouter_cache_read_pricing_over_incomplete_litellm).
    let mut without_cache_read = pricing;
    without_cache_read.cache_read_input_token_cost = None;
    without_cache_read.cache_read_input_token_cost_above_272k_tokens = None;
    assert!(!has_complete_openai_272k_pricing(&without_cache_read));
}

#[test]
fn openai_272k_preference_prefers_openrouter_cache_read_pricing_over_incomplete_litellm() {
    let mut litellm_pricing = openai_272k_result("gpt-5.6-sol", "LiteLLM").pricing;
    litellm_pricing.cache_read_input_token_cost = None;
    litellm_pricing.cache_read_input_token_cost_above_272k_tokens = None;

    let openrouter_pricing = openai_272k_result("openai/gpt-5.6-sol", "OpenRouter").pricing;

    let lookup = PricingLookup::new(
        HashMap::from([("gpt-5.6-sol".into(), litellm_pricing)]),
        HashMap::from([("openai/gpt-5.6-sol".into(), openrouter_pricing)]),
        HashMap::new(),
    );

    let result = lookup
        .lookup_with_provider("gpt-5.6-sol", Some("openai"))
        .unwrap();
    assert_eq!(result.source, "OpenRouter");
    assert_eq!(result.matched_key, "openai/gpt-5.6-sol");
    assert!(result.pricing.cache_read_input_token_cost.is_some());
}

#[test]
fn openai_272k_preference_still_prefers_complete_litellm_pricing() {
    let litellm_pricing = openai_272k_result("gpt-5.6-sol", "LiteLLM").pricing;
    let openrouter_pricing = openai_272k_result("openai/gpt-5.6-sol", "OpenRouter").pricing;

    let lookup = PricingLookup::new(
        HashMap::from([("gpt-5.6-sol".into(), litellm_pricing)]),
        HashMap::from([("openai/gpt-5.6-sol".into(), openrouter_pricing)]),
        HashMap::new(),
    );

    let result = lookup
        .lookup_with_provider("gpt-5.6-sol", Some("openai"))
        .unwrap();
    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.matched_key, "gpt-5.6-sol");
}

#[test]
fn test_openai_272k_full_request_pricing_scope() {
    for key in [
        "gpt-5.4",
        "openai/gpt-5.4-pro-2026-03-05",
        "gpt-5.5-2026-04-23",
        "gpt-5.5-pro",
        "gpt-5.5-pro-2026-04-23",
        "gpt-5.6",
        "gpt-5.6-sol",
        "gpt-5.6-terra-2026-07-01",
        "gpt-5.6-luna",
    ] {
        assert!(
            uses_openai_full_request_272k_pricing(
                &openai_272k_result(key, "LiteLLM"),
                Some("openai")
            ),
            "expected full-request pricing for {key}"
        );
    }

    for key in [
        "gpt-5.4-mini",
        "gpt-5.4-nano",
        "gpt-5.5-promax",
        "gpt-5.2",
        "fugu-ultra",
        "custom/gpt-5.5-pro",
    ] {
        assert!(
            !uses_openai_full_request_272k_pricing(
                &openai_272k_result(key, "LiteLLM"),
                Some("openai")
            ),
            "expected progressive pricing for {key}"
        );
    }

    for (result, provider) in [
        (openai_272k_result("fugu-ultra", "LiteLLM"), None),
        (openai_272k_result("openai/gpt-5.5", "OpenRouter"), None),
        (
            openai_272k_result("azure/openai/gpt-5.5", "LiteLLM"),
            Some("azure"),
        ),
    ] {
        assert!(!uses_openai_full_request_272k_pricing(&result, provider));
    }
}

#[test]
fn orcarouter_hint_keeps_litellm_fallback_on_progressive_long_context_pricing() {
    // OrcaRouter can fall back to LiteLLM's unscoped OpenAI row when its
    // provider-specific catalog has no match. The provider hint, not an
    // invented OrcaRouter LiteLLM key, must keep that fallback on normal
    // progressive tiers instead of applying direct-OpenAI full-request
    // 272K semantics.
    let result = openai_272k_result("gpt-5.5", "LiteLLM");
    let usage = TokenBreakdown {
        input: 200_000,
        output: 10_000,
        cache_read: 72_001,
        ..Default::default()
    };

    assert!(uses_openai_full_request_272k_pricing(
        &result,
        Some("openai")
    ));
    assert!(!uses_openai_full_request_272k_pricing(
        &result,
        Some("orcarouter")
    ));

    let direct_openai_cost = compute_cost_for_lookup(&result, Some("openai"), &usage);
    let direct_openai_expected =
        (200_000.0 * 0.000010) + (10_000.0 * 0.000045) + (72_001.0 * 0.000001);
    assert!((direct_openai_cost - direct_openai_expected).abs() < 1e-12);

    let orcarouter_cost = compute_cost_for_lookup(&result, Some("orcarouter"), &usage);
    let orcarouter_expected =
        (200_000.0 * 0.000005) + (10_000.0 * 0.000030) + (72_001.0 * 0.0000005);
    assert!((orcarouter_cost - orcarouter_expected).abs() < 1e-12);
}
