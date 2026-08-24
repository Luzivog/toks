use super::*;

#[test]
fn test_sakana_returns_pricing_for_fugu_ultra() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service.lookup_with_source("fugu-ultra", None).unwrap();
    assert_eq!(result.source, "Sakana");
    assert_eq!(result.matched_key, "fugu-ultra");
    assert_eq!(result.pricing.input_cost_per_token, Some(5e-6));
    assert_eq!(result.pricing.output_cost_per_token, Some(3e-5));
    assert_eq!(result.pricing.cache_read_input_token_cost, Some(5e-7));
    assert_eq!(result.pricing.cache_creation_input_token_cost, None);
    // >272K tier fields are populated (compute_cost reads them).
    assert_eq!(
        result.pricing.input_cost_per_token_above_272k_tokens,
        Some(1e-5)
    );
    assert_eq!(
        result.pricing.output_cost_per_token_above_272k_tokens,
        Some(4.5e-5)
    );
    assert_eq!(
        result.pricing.cache_read_input_token_cost_above_272k_tokens,
        Some(1e-6)
    );
}

#[test]
fn test_sakana_calculate_cost_for_fugu_ultra() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    // Stay under the 272K threshold so only base rates apply.
    let cost = service.calculate_cost("fugu-ultra", 100_000, 10_000, 50_000, 0, 0);
    let expected = 100_000.0 * 5e-6 + 10_000.0 * 3e-5 + 50_000.0 * 5e-7;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn test_sakana_yields_to_litellm_exact() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fugu-ultra".into(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let result = service.lookup_with_source("fugu-ultra", None).unwrap();
    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.001));
}

#[test]
fn test_sakana_does_not_price_bare_fugu() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    // Bare `fugu` is a router/orchestrator — deliberately unpriced by Sakana.
    let result = service.lookup_with_source("fugu", None);
    assert!(
        result.as_ref().is_none_or(|r| r.source != "Sakana"),
        "bare `fugu` must not resolve to a Sakana price"
    );
}

#[test]
fn test_sakana_resolves_dated_fugu_ultra_alias() {
    let service = PricingService::new(HashMap::new(), HashMap::new());
    let result = service
        .lookup_with_source("fugu-ultra-20260615", None)
        .unwrap();
    assert_eq!(result.source, "Sakana");
    assert_eq!(result.matched_key, "fugu-ultra");
    assert_eq!(result.pricing.input_cost_per_token, Some(5e-6));
}

#[test]
fn embedded_baseline_is_available_without_any_disk_cache() {
    let service = PricingService::from_cached_datasets(None, None, None)
        .expect("embedded pricing must make offline startup deterministic");
    let result = service
        .lookup_with_source_and_provider("gpt-5.6-sol", None, Some("openai"))
        .expect("current Codex model must be priced offline");

    assert_eq!(result.source, "Models.dev");
    assert_eq!(result.matched_key, "openai/gpt-5.6-sol");
    assert_eq!(result.pricing.input_cost_per_token, Some(5e-6));
}

#[test]
fn cached_catalog_overrides_embedded_baseline() {
    let mut cached = HashMap::new();
    cached.insert("openai/gpt-5.6-sol".to_string(), model_pricing(9e-6, 10e-6));
    let service = PricingService::from_cached_datasets(None, None, Some(cached)).unwrap();
    let result = service
        .lookup_with_source_and_provider("gpt-5.6-sol", None, Some("openai"))
        .unwrap();

    assert_eq!(result.pricing.input_cost_per_token, Some(9e-6));
}

#[test]
fn test_from_cached_datasets_filters_subscription_only_litellm_entries() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "github_copilot/gpt-5.3-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    litellm.insert(
        "gpt-5.2".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000175),
            ..Default::default()
        },
    );

    let service = PricingService::from_cached_datasets(Some(litellm), None, None).unwrap();

    assert!(service
        .lookup_with_source("github_copilot/gpt-5.3-codex", Some("litellm"))
        .is_none());
    assert!(service
        .lookup_with_source("gpt-5.2", Some("litellm"))
        .is_some());
}

#[test]
fn test_from_cached_datasets_uses_models_dev_when_other_sources_missing() {
    let service =
        PricingService::from_cached_datasets(None, None, Some(fixture_models_dev())).unwrap();

    let result = service
        .lookup_with_source_and_provider("gpt-fixture-model", None, Some("openai"))
        .unwrap();

    assert_eq!(result.source, "Models.dev");
    assert_eq!(result.matched_key, "openai/gpt-fixture-model");
}
