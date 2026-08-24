use super::*;

// Regression: #1013. Submission validation judged bucket coverage against
// the provider-hinted row alone. For `openai/gpt-5.2-codex` the hint lands
// on an OpenRouter row with no cache-read rate while the canonical LiteLLM
// row publishes one, so every Codex session — which always carries cached
// tokens — was reported as unpriced and aborted the whole submission.
#[test]
fn hinted_row_missing_a_cache_rate_still_covers_usage() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "azure/codex-cache-gap".to_string(),
        ModelPricing {
            input_cost_per_token: Some(1.75e-6),
            output_cost_per_token: Some(1.4e-5),
            ..Default::default()
        },
    );
    litellm.insert(
        "codex-cache-gap".to_string(),
        ModelPricing {
            input_cost_per_token: Some(1.75e-6),
            output_cost_per_token: Some(1.4e-5),
            cache_read_input_token_cost: Some(1.75e-7),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let usage = cache_read_usage();

    assert!(service.covers_usage_with_provider("codex-cache-gap", Some("azure"), &usage));
    let cost = service.calculate_cost_with_provider("codex-cache-gap", Some("azure"), &usage);
    assert!((cost - 1.925).abs() < 1e-9, "unexpected cost: {cost}");
}

// Regression: #1021, #1035. The unit tests around `covers_usage` pin the
// row-level rule; this pins the behaviour the issues actually reported,
// which is a submission aborting. It has to run through `PricingService`
// because the shortcut is only reached via `resolve_for_usage`, whose
// `normalize_provider_hint(..).is_none() || covers_usage(..)` condition
// decides whether a hinted row is consulted at all — reordering that
// condition would reintroduce the aborted submission while every
// row-level test kept passing.
#[test]
fn a_hinted_all_zero_row_covers_cache_usage_through_the_service() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "kenari/nemotron-free-fixture".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let usage = cache_read_usage();

    assert!(
        service.covers_usage_with_provider("nemotron-free-fixture", Some("kenari"), &usage),
        "cache-bearing usage on an all-zero row must not abort the submission"
    );
    let cost =
        service.calculate_cost_with_provider("nemotron-free-fixture", Some("kenari"), &usage);
    assert_eq!(cost, 0.0, "an all-zero row must price at exactly zero");
}

#[test]
fn reasonix_uses_the_inferred_upstream_provider_for_pricing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "deepseek/reasonix-fixture".to_string(),
        ModelPricing {
            input_cost_per_token: Some(2e-6),
            output_cost_per_token: Some(8e-6),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let usage = TokenBreakdown {
        input: 1_000,
        output: 1_000,
        ..Default::default()
    };

    assert!(service.covers_usage_with_provider(
        "opencode/reasonix-fixture",
        Some("deepseek"),
        &usage,
    ));
    assert!(
        (service.calculate_cost_with_provider(
            "opencode/reasonix-fixture",
            Some("deepseek"),
            &usage,
        ) - 0.01)
            .abs()
            < 1e-12
    );
}

// The two rows must be the same deal before one lends the other a rate.
// `azure_ai/grok-code-fast-1` bills $3.50/$17.50 per million with no
// cache-read rate while the canonical `xai/` row bills $0.20/$1.50 with
// one; borrowing across them would invent an Azure-base, xAI-cache tariff
// that neither provider charges.
#[test]
fn differently_priced_canonical_row_does_not_lend_its_cache_rate() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "azure/grok-tariff-guard".to_string(),
        ModelPricing {
            input_cost_per_token: Some(3.5e-6),
            output_cost_per_token: Some(1.75e-5),
            ..Default::default()
        },
    );
    litellm.insert(
        "grok-tariff-guard".to_string(),
        ModelPricing {
            input_cost_per_token: Some(2e-7),
            output_cost_per_token: Some(1.5e-6),
            cache_read_input_token_cost: Some(2e-8),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let usage = cache_read_usage();

    assert!(
        !service.covers_usage_with_provider("grok-tariff-guard", Some("azure"), &usage),
        "a differently priced row must not make the usage look priceable"
    );
    let cost = service.calculate_cost_with_provider("grok-tariff-guard", Some("azure"), &usage);
    assert!(
        (cost - 3.5).abs() < 1e-9,
        "the reseller's own rates must be the only ones applied: {cost}"
    );
}

// Guard for the fix above: borrowing must never reach a bucket the hinted
// row already prices, otherwise a reseller row (e.g. `azure_ai/` at a
// markup over `xai/`) would silently reprice to the author's cheaper rate.
#[test]
fn covered_hinted_row_is_not_replaced_by_the_canonical_row() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "azure/marked-up-model".to_string(),
        ModelPricing {
            input_cost_per_token: Some(1e-5),
            cache_read_input_token_cost: Some(1e-6),
            ..Default::default()
        },
    );
    litellm.insert(
        "marked-up-model".to_string(),
        ModelPricing {
            input_cost_per_token: Some(1e-7),
            cache_read_input_token_cost: Some(1e-8),
            ..Default::default()
        },
    );
    let service = PricingService::new(litellm, HashMap::new());
    let usage = cache_read_usage();

    assert!(service.covers_usage_with_provider("marked-up-model", Some("azure"), &usage));
    let cost = service.calculate_cost_with_provider("marked-up-model", Some("azure"), &usage);
    assert!(
        (cost - 11.0).abs() < 1e-9,
        "reseller markup must survive: {cost}"
    );
}

// A model nothing can price must still be rejected, so submissions never
// silently bill genuinely unknown usage at zero.
#[test]
fn usage_stays_uncovered_when_no_resolution_prices_the_bucket() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "azure/no-cache-anywhere".to_string(),
        model_pricing(1e-5, 1e-4),
    );
    litellm.insert("no-cache-anywhere".to_string(), model_pricing(1e-6, 1e-5));
    let service = PricingService::new(litellm, HashMap::new());

    assert!(!service.covers_usage_with_provider(
        "no-cache-anywhere",
        Some("azure"),
        &cache_read_usage()
    ));
}

// Custom overrides are exact-only and provider-agnostic, so they must be
// consulted before any provider-hinted resolution or bucket borrowing.
#[test]
fn custom_pricing_decides_coverage_before_any_fallback() {
    let mut custom = HashMap::new();
    custom.insert(
        "custom-covered-model".to_string(),
        ModelPricing {
            input_cost_per_token: Some(1e-6),
            cache_read_input_token_cost: Some(1e-7),
            ..Default::default()
        },
    );
    let service = custom_service(custom, HashMap::new(), HashMap::new());

    assert!(service.covers_usage_with_provider(
        "custom-covered-model",
        Some("azure"),
        &cache_read_usage()
    ));
}
