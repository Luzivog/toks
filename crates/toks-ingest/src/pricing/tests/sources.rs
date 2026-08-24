use super::*;

// Regression: #1002. A LiteLLM fetch failure used to propagate out of
// fetch_inner, so `tokscope submit` died with "error decoding response
// body" even though models.dev and openrouter were both reachable and
// carried usable pricing.
#[test]
fn litellm_fetch_failure_is_not_fatal_when_another_source_has_data() {
    let mut models_dev = HashMap::new();
    models_dev.insert("test-model-alpha".to_string(), model_pricing(1e-6, 2e-6));

    let service = PricingService::combine_fetched_sources(
        Err("error decoding response body".to_string()),
        Err("OpenRouter unavailable".to_string()),
        Ok(models_dev),
        // Fresh install, as in the report: nothing cached yet.
        || None,
        || None,
        || None,
        CustomPricing::default(),
    )
    .expect("a LiteLLM failure must not be fatal while another source has pricing");

    let cost = service.calculate_cost("test-model-alpha", 1_000_000, 0, 0, 0, 0);
    assert!(
        (cost - 1.0).abs() < 1e-9,
        "models.dev pricing should still resolve after LiteLLM fails, got {}",
        cost
    );
}

// Regression: #1002. The reporter's workaround was hand-populating the
// cache file. A cached copy older than the 1h TTL must be preferred over
// dropping LiteLLM entirely, so that workaround keeps working unattended.
#[test]
fn litellm_fetch_failure_falls_back_to_stale_cache() {
    let mut cached = HashMap::new();
    cached.insert("test-model-beta".to_string(), model_pricing(3e-6, 4e-6));

    let service = PricingService::combine_fetched_sources(
        Err("error decoding response body".to_string()),
        Err("OpenRouter unavailable".to_string()),
        Ok(HashMap::new()),
        || Some(cached),
        || None,
        || None,
        CustomPricing::default(),
    )
    .expect("a stale LiteLLM cache must keep the service usable");

    let cost = service.calculate_cost("test-model-beta", 1_000_000, 0, 0, 0, 0);
    assert!(
        (cost - 3.0).abs() < 1e-9,
        "stale LiteLLM cache should price the model, got {}",
        cost
    );
}

// Regression: models.dev is a degradable source too. Its errors used to be
// dropped straight to an empty map even though it keeps a cache of its own,
// so a models.dev outage discarded pricing that was sitting on disk.
#[test]
fn models_dev_fetch_failure_falls_back_to_stale_cache() {
    let mut cached = HashMap::new();
    cached.insert("test-model-gamma".to_string(), model_pricing(5e-6, 6e-6));

    let service = PricingService::combine_fetched_sources(
        Ok(HashMap::new()),
        Err("OpenRouter unavailable".to_string()),
        Err("models.dev unreachable".to_string()),
        || None,
        || None,
        || Some(cached),
        CustomPricing::default(),
    )
    .expect("a stale models.dev cache must keep the service usable");

    let cost = service.calculate_cost("test-model-gamma", 1_000_000, 0, 0, 0, 0);
    assert!(
        (cost - 5.0).abs() < 1e-9,
        "stale models.dev cache should price the model, got {}",
        cost
    );
}

#[test]
fn custom_pricing_keeps_service_available_during_dynamic_outage() {
    let mut custom = HashMap::new();
    custom.insert("custom-only".to_string(), model_pricing(3e-6, 4e-6));
    let service = PricingService::combine_fetched_sources(
        Err("error decoding response body: expected f64".to_string()),
        Err("OpenRouter unreachable".to_string()),
        Err("models.dev unreachable".to_string()),
        || None,
        || None,
        || None,
        CustomPricing::from_models(custom),
    )
    .expect("custom pricing should remain usable during an upstream outage");
    assert!(service.lookup_with_source("custom-only", None).is_some());
}

#[test]
fn openrouter_fetch_failure_falls_back_to_stale_cache() {
    let mut cached = HashMap::new();
    cached.insert("openrouter-only".to_string(), model_pricing(7e-6, 8e-6));

    let service = PricingService::combine_fetched_sources(
        Err("LiteLLM unavailable".to_string()),
        Err("OpenRouter unavailable".to_string()),
        Err("models.dev unavailable".to_string()),
        || None,
        || Some(cached),
        || None,
        CustomPricing::default(),
    )
    .expect("a stale OpenRouter cache must keep the service usable");

    assert!(service
        .lookup_with_source("openrouter-only", None)
        .is_some());
}

#[test]
fn models_dev_parses_fixture_prices_per_token() {
    let data = fixture_models_dev();
    let pricing = data.get("openai/gpt-fixture-model").unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(0.00000125));
    assert_eq!(pricing.output_cost_per_token, Some(0.00001));
    assert_eq!(pricing.cache_read_input_token_cost, Some(0.000000125));
    assert_eq!(pricing.cache_creation_input_token_cost, Some(0.000001875));
    assert!(!data.contains_key("openai/missing-output-price"));
}

#[test]
fn models_dev_fills_provider_aware_fallback_prices() {
    let service = custom_service_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        fixture_models_dev(),
    );

    let result = service
        .lookup_with_source_and_provider("gpt-fixture-model", None, Some("openai"))
        .unwrap();

    assert_eq!(result.source, "Models.dev");
    assert_eq!(result.matched_key, "openai/gpt-fixture-model");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.00000125));
}

#[test]
fn models_dev_cache_prices_are_used_for_cost_fallback() {
    let service = custom_service_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        fixture_models_dev(),
    );
    let usage = TokenBreakdown {
        input: 1_000_000,
        output: 100_000,
        cache_read: 50_000,
        cache_write: 20_000,
        reasoning: 0,
    };

    let cost = service.calculate_cost_with_provider("gpt-fixture-model", Some("openai"), &usage);

    let expected = 1.25 + 1.0 + 0.00625 + 0.0375;
    assert!((cost - expected).abs() < 1e-10);
}

#[test]
fn existing_sources_beat_models_dev_fallback() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-fixture-model".into(),
        model_pricing(0.000002, 0.000008),
    );
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-fixture-sonnet".into(),
        model_pricing(0.000004, 0.000016),
    );

    let service =
        custom_service_with_models_dev(HashMap::new(), litellm, openrouter, fixture_models_dev());

    let litellm_result = service
        .lookup_with_source_and_provider("gpt-fixture-model", None, Some("openai"))
        .unwrap();
    assert_eq!(litellm_result.source, "LiteLLM");
    assert_eq!(litellm_result.pricing.input_cost_per_token, Some(0.000002));

    let openrouter_result = service
        .lookup_with_source_and_provider("claude-fixture-sonnet", None, Some("anthropic"))
        .unwrap();
    assert_eq!(openrouter_result.source, "OpenRouter");
    assert_eq!(
        openrouter_result.pricing.input_cost_per_token,
        Some(0.000004)
    );
}

#[test]
fn models_dev_respects_forced_source_boundaries() {
    let service = custom_service_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        fixture_models_dev(),
    );

    assert!(service
        .lookup_with_source_and_provider("gpt-fixture-model", Some("litellm"), Some("openai"))
        .is_none());
    assert!(service
        .lookup_with_source_and_provider("gpt-fixture-model", Some("openrouter"), Some("openai"))
        .is_none());

    let result = service
        .lookup_with_source_and_provider("gpt-fixture-model", Some("models.dev"), Some("openai"))
        .unwrap();
    assert_eq!(result.source, "Models.dev");
}

#[test]
fn custom_override_beats_models_dev_fallback() {
    let mut custom = HashMap::new();
    custom.insert(
        "gpt-fixture-model".into(),
        model_pricing(0.000009, 0.000018),
    );

    let service = custom_service_with_models_dev(
        custom,
        HashMap::new(),
        HashMap::new(),
        fixture_models_dev(),
    );

    let result = service
        .lookup_with_source_and_provider("gpt-fixture-model", None, Some("openai"))
        .unwrap();

    assert_eq!(result.source, "Custom");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.000009));
}

#[test]
fn test_filter_excludes_github_copilot() {
    let mut data = HashMap::new();
    data.insert(
        "github_copilot/gpt-5.3-codex".into(),
        ModelPricing::default(),
    );
    data.insert("github_copilot/gpt-4o".into(), ModelPricing::default());
    data.insert(
        "gpt-5.2".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000175),
            ..Default::default()
        },
    );
    data.insert(
        "openai/gpt-5.2".into(),
        ModelPricing {
            output_cost_per_token: Some(0.000014),
            ..Default::default()
        },
    );
    data.insert(
        "tier-only".into(),
        ModelPricing {
            input_cost_per_token_above_272k_tokens: Some(0.00001),
            ..Default::default()
        },
    );

    let filtered = PricingService::filter_litellm_data(data);
    assert!(!filtered.contains_key("github_copilot/gpt-5.3-codex"));
    assert!(!filtered.contains_key("github_copilot/gpt-4o"));
    assert!(filtered.contains_key("gpt-5.2"));
    assert!(filtered.contains_key("openai/gpt-5.2"));
    assert!(!filtered.contains_key("tier-only"));
}
