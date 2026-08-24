use super::*;

#[test]
fn test_cursor_returns_pricing_when_not_in_upstream() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("gpt-5.3-codex", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.00000175));
    assert_eq!(result.pricing.output_cost_per_token, Some(0.000014));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(1.75e-7));
}

#[test]
fn test_cursor_yields_to_litellm_exact() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-5.3-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.002),
            output_cost_per_token: Some(0.016),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let result = service.lookup_with_source("gpt-5.3-codex", None).unwrap();
    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.002));
}

#[test]
fn test_cursor_yields_to_openrouter_prefix() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "openai/gpt-5.3-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.003),
            output_cost_per_token: Some(0.012),
            ..Default::default()
        },
    );
    let service = PricingService::new(HashMap::new(), openrouter);
    let result = service.lookup_with_source("gpt-5.3-codex", None).unwrap();
    assert_eq!(result.source, "OpenRouter");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.003));
}

#[test]
fn test_cursor_skipped_when_force_source_set() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    assert!(service
        .lookup_with_source("gpt-5.3-codex", Some("litellm"))
        .is_none());
    assert!(service
        .lookup_with_source("gpt-5.3-codex", Some("openrouter"))
        .is_none());
}

#[test]
fn test_cursor_matches_after_version_normalization() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("gpt-5-3-codex", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "gpt-5.3-codex");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.00000175));
}

#[test]
fn test_cursor_matches_provider_prefixed_input() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service
        .lookup_with_source("openai/gpt-5.3-codex", None)
        .unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "gpt-5.3-codex");
}

#[test]
fn test_cursor_provider_prefix_yields_to_upstream() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "openai/gpt-5.3-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.003),
            output_cost_per_token: Some(0.012),
            ..Default::default()
        },
    );
    let service = PricingService::new(HashMap::new(), openrouter);
    let result = service
        .lookup_with_source("openai/gpt-5.3-codex", None)
        .unwrap();
    assert_eq!(result.source, "OpenRouter");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.003));
}

#[test]
fn test_cursor_matches_via_suffix_stripping() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service
        .lookup_with_source("gpt-5.3-codex-high", None)
        .unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "gpt-5.3-codex");
}

#[test]
fn test_cursor_calculate_cost() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("gpt-5.3-codex", 1_000_000, 100_000, 0, 0, 0);
    let expected = 1_000_000.0 * 0.00000175 + 100_000.0 * 0.000014;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_returns_pricing_for_composer_1() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("Composer 1", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer 1");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.00000125));
    assert_eq!(result.pricing.output_cost_per_token, Some(0.00001));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(1.25e-7));
}

#[test]
fn test_cursor_calculate_cost_for_composer_1() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("Composer 1", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 0.00000125 + 100_000.0 * 0.00001 + 50_000.0 * 1.25e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_returns_pricing_for_hyphenated_composer_1() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("composer-1", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-1");
}

#[test]
fn test_cursor_returns_pricing_for_composer_1_5() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("Composer 1.5", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer 1.5");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.0000035));
    assert_eq!(result.pricing.output_cost_per_token, Some(0.0000175));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(3.5e-7));
}

#[test]
fn test_cursor_calculate_cost_for_composer_1_5() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("Composer 1.5", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 0.0000035 + 100_000.0 * 0.0000175 + 50_000.0 * 3.5e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_returns_pricing_for_hyphenated_composer_1_5() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("composer-1.5", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-1.5");
}

#[test]
fn test_cursor_returns_pricing_for_composer_2() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("composer-2", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-2");
    assert_eq!(result.pricing.input_cost_per_token, Some(5e-7));
    assert_eq!(result.pricing.output_cost_per_token, Some(2.5e-6));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(2e-7));
    // Cursor documents cache creation as FREE for the Composer 2 family.
    // Some(0.0) and None compute the same cost, but only Some(0.0)
    // makes covers_usage accept cache_write usage for submission.
    assert_eq!(result.pricing.cache_creation_input_token_cost, Some(0.0));
}

#[test]
fn test_cursor_returns_pricing_for_composer_2_spaced() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("Composer 2", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer 2");
}

#[test]
fn test_cursor_returns_pricing_for_composer_2_fast() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("composer-2-fast", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-2-fast");
    assert_eq!(result.pricing.input_cost_per_token, Some(1.5e-6));
    assert_eq!(result.pricing.output_cost_per_token, Some(7.5e-6));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(3.5e-7));
    // Cursor documents cache creation as FREE for the Composer 2 family.
    // Some(0.0) and None compute the same cost, but only Some(0.0)
    // makes covers_usage accept cache_write usage for submission.
    assert_eq!(result.pricing.cache_creation_input_token_cost, Some(0.0));
}

#[test]
fn test_cursor_returns_pricing_for_composer_2_fast_spaced() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("Composer 2 Fast", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer 2 fast");
}

#[test]
fn test_cursor_calculate_cost_for_composer_2() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("composer-2", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 5e-7 + 100_000.0 * 2.5e-6 + 50_000.0 * 2e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_composer_2_cache_write_free() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let with_write = service.calculate_cost("composer-2", 0, 0, 0, 500_000, 0);
    let without_write = service.calculate_cost("composer-2", 0, 0, 0, 0, 0);
    assert!((with_write - without_write).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_for_composer_2_fast() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("composer-2-fast", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 1.5e-6 + 100_000.0 * 7.5e-6 + 50_000.0 * 3.5e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_composer_2_fast_cache_write_free() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let with_write = service.calculate_cost("composer-2-fast", 0, 0, 0, 500_000, 0);
    let without_write = service.calculate_cost("composer-2-fast", 0, 0, 0, 0, 0);
    assert!(
        (with_write - without_write).abs() < 1e-10,
        "Cache creation should be free for Composer 2 Fast"
    );
}

#[test]
fn test_cursor_returns_pricing_for_composer_2_5() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("composer-2.5", None).unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-2.5");
    assert_eq!(result.pricing.input_cost_per_token, Some(5e-7));
    assert_eq!(result.pricing.output_cost_per_token, Some(2.5e-6));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(2e-7));
    // Cursor documents cache creation as FREE for the Composer 2 family.
    // Some(0.0) and None compute the same cost, but only Some(0.0)
    // makes covers_usage accept cache_write usage for submission.
    assert_eq!(result.pricing.cache_creation_input_token_cost, Some(0.0));
}

#[test]
fn test_cursor_returns_pricing_for_composer_2_5_fast() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service
        .lookup_with_source("composer-2.5-fast", None)
        .unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-2.5-fast");
    assert_eq!(result.pricing.input_cost_per_token, Some(1.5e-6));
    assert_eq!(result.pricing.output_cost_per_token, Some(7.5e-6));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(3.5e-7));
    // Cursor documents cache creation as FREE for the Composer 2 family.
    // Some(0.0) and None compute the same cost, but only Some(0.0)
    // makes covers_usage accept cache_write usage for submission.
    assert_eq!(result.pricing.cache_creation_input_token_cost, Some(0.0));
}

#[test]
fn test_grok_composer_2_5_fast_uses_composer_2_5_fast_override() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service
        .lookup_with_source("grok-composer-2.5-fast", None)
        .unwrap();
    assert_eq!(result.source, "Cursor");
    assert_eq!(result.matched_key, "composer-2.5-fast");
    assert_eq!(result.pricing.input_cost_per_token, Some(1.5e-6));
    assert_eq!(result.pricing.output_cost_per_token, Some(7.5e-6));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(3.5e-7));

    let cost = service.calculate_cost("grok-composer-2.5-fast", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 1.5e-6 + 100_000.0 * 7.5e-6 + 50_000.0 * 3.5e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_for_composer_2_5() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("composer-2.5", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 5e-7 + 100_000.0 * 2.5e-6 + 50_000.0 * 2e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_composer_2_5_cache_write_free() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let with_write = service.calculate_cost("composer-2.5", 0, 0, 0, 500_000, 0);
    let without_write = service.calculate_cost("composer-2.5", 0, 0, 0, 0, 0);
    assert!((with_write - without_write).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_for_composer_2_5_fast() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let cost = service.calculate_cost("composer-2.5-fast", 1_000_000, 100_000, 50_000, 0, 0);
    let expected = 1_000_000.0 * 1.5e-6 + 100_000.0 * 7.5e-6 + 50_000.0 * 3.5e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_cursor_calculate_cost_composer_2_5_fast_cache_write_free() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let with_write = service.calculate_cost("composer-2.5-fast", 0, 0, 0, 500_000, 0);
    let without_write = service.calculate_cost("composer-2.5-fast", 0, 0, 0, 0, 0);
    assert!(
        (with_write - without_write).abs() < 1e-10,
        "Cache creation should be free for Composer 2.5 Fast"
    );
}

#[test]
fn test_cursor_composer_lookup_case_insensitive() {
    let service = PricingService::new(HashMap::new(), HashMap::new());

    let lower = service.lookup_with_source("composer 1", None);
    let upper = service.lookup_with_source("COMPOSER 1", None);
    let mixed = service.lookup_with_source("Composer 1", None);

    assert!(lower.is_some(), "lowercase should resolve");
    assert!(upper.is_some(), "UPPERCASE should resolve");
    assert!(mixed.is_some(), "Mixed Case should resolve");

    assert_eq!(
        lower.unwrap().pricing.input_cost_per_token,
        upper.unwrap().pricing.input_cost_per_token
    );
}

/// Regression: Composer 2's cache creation is documented FREE, but it was
/// encoded as `None` ("rate unknown"), so `covers_usage` reported the row
/// as not covering any usage with cache_write and submission excluded it.
/// The cost is unchanged either way — `compute_cost` reads an absent rate
/// as 0.0 — so this is purely about the coverage verdict.
#[test]
fn cursor_documented_free_cache_creation_covers_cache_write_usage() {
    let overrides = PricingService::build_cursor_overrides();
    let usage = crate::TokenBreakdown {
        input: 1_000,
        output: 500,
        cache_read: 200,
        cache_write: 300,
        ..Default::default()
    };

    let composer2 = overrides.get("composer-2").expect("composer-2 override");
    assert_eq!(composer2.cache_creation_input_token_cost, Some(0.0));
    assert!(
        composer2.covers_usage(&usage),
        "documented-free cache creation must count as covered"
    );

    // Composer 1 has no documented cache-creation rate, so it stays unknown
    // rather than being guessed at zero. Excluding it is the honest answer.
    let composer1 = overrides.get("composer-1").expect("composer-1 override");
    assert_eq!(composer1.cache_creation_input_token_cost, None);
    assert!(!composer1.covers_usage(&usage));
}
