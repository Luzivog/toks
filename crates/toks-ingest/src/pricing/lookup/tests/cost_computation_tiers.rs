use super::super::*;
use super::create_lookup;

#[test]
fn test_compute_cost_tiered_is_applied_per_bucket() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.000001,
            "input_cost_per_token_above_200k_tokens": 0.000002,
            "output_cost_per_token": 0.000003,
            "output_cost_per_token_above_200k_tokens": 0.000004
        }"#,
    )
    .unwrap();

    let cost = compute_cost(&pricing, 200_001, 200_000, 0, 0, 0);
    let expected = (200_000.0 * 0.000001 + 1.0 * 0.000002) + (200_000.0 * 0.000003);

    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_missing_base_input_only_charges_above_threshold() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token_above_200k_tokens": 0.000002
        }"#,
    )
    .unwrap();

    let at_threshold = compute_cost(&pricing, 200_000, 0, 0, 0, 0);
    let above_threshold = compute_cost(&pricing, 200_001, 0, 0, 0, 0);

    assert_eq!(at_threshold, 0.0);
    assert!((above_threshold - 0.000002).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_cache_read_applies_split() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "cache_read_input_token_cost": 0.0000001,
            "cache_read_input_token_cost_above_200k_tokens": 0.0000002
        }"#,
    )
    .unwrap();

    let at_threshold = compute_cost(&pricing, 0, 0, 200_000, 0, 0);
    let above_threshold = compute_cost(&pricing, 0, 0, 200_001, 0, 0);

    assert!((at_threshold - (200_000.0 * 0.0000001)).abs() < 1e-12);
    assert!((above_threshold - (200_000.0 * 0.0000001 + 0.0000002)).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_cache_write_applies_split() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "cache_creation_input_token_cost": 0.0000003,
            "cache_creation_input_token_cost_above_200k_tokens": 0.0000004
        }"#,
    )
    .unwrap();

    let at_threshold = compute_cost(&pricing, 0, 0, 0, 200_000, 0);
    let above_threshold = compute_cost(&pricing, 0, 0, 0, 200_001, 0);

    assert!((at_threshold - (200_000.0 * 0.0000003)).abs() < 1e-12);
    assert!((above_threshold - (200_000.0 * 0.0000003 + 0.0000004)).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_without_above_rate_uses_base_for_all_tokens() {
    let pricing = ModelPricing {
        input_cost_per_token: Some(0.000001),
        ..Default::default()
    };

    let cost = compute_cost(&pricing, 250_000, 0, 0, 0, 0);

    assert!((cost - (250_000.0 * 0.000001)).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_invalid_above_rate_falls_back_to_base() {
    let pricing_negative = ModelPricing {
        input_cost_per_token: Some(0.000001),
        input_cost_per_token_above_200k_tokens: Some(-0.000002),
        ..Default::default()
    };
    let pricing_infinite = ModelPricing {
        input_cost_per_token: Some(0.000001),
        input_cost_per_token_above_200k_tokens: Some(f64::INFINITY),
        ..Default::default()
    };
    let pricing_nan = ModelPricing {
        input_cost_per_token: Some(0.000001),
        input_cost_per_token_above_200k_tokens: Some(f64::NAN),
        ..Default::default()
    };

    let expected = 200_001.0 * 0.000001;
    assert!((compute_cost(&pricing_negative, 200_001, 0, 0, 0, 0) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_infinite, 200_001, 0, 0, 0, 0) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_nan, 200_001, 0, 0, 0, 0) - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_reasoning_boundary_at_200k_uses_base_output_rate() {
    let pricing = ModelPricing {
        output_cost_per_token: Some(0.000003),
        output_cost_per_token_above_200k_tokens: Some(0.000004),
        ..Default::default()
    };

    let cost = compute_cost(&pricing, 0, 199_999, 0, 0, 1);
    let expected = 200_000.0 * 0.000003;

    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_invalid_above_rate_falls_back_to_base_output_reasoning() {
    let pricing_negative = ModelPricing {
        output_cost_per_token: Some(0.000003),
        output_cost_per_token_above_200k_tokens: Some(-0.000004),
        ..Default::default()
    };
    let pricing_infinite = ModelPricing {
        output_cost_per_token: Some(0.000003),
        output_cost_per_token_above_200k_tokens: Some(f64::INFINITY),
        ..Default::default()
    };
    let pricing_nan = ModelPricing {
        output_cost_per_token: Some(0.000003),
        output_cost_per_token_above_200k_tokens: Some(f64::NAN),
        ..Default::default()
    };

    let expected = 200_001.0 * 0.000003;
    assert!((compute_cost(&pricing_negative, 0, 199_999, 0, 0, 2) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_infinite, 0, 199_999, 0, 0, 2) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_nan, 0, 199_999, 0, 0, 2) - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_invalid_above_rate_falls_back_to_base_cache_read() {
    let pricing_negative = ModelPricing {
        cache_read_input_token_cost: Some(0.0000001),
        cache_read_input_token_cost_above_200k_tokens: Some(-0.0000002),
        ..Default::default()
    };
    let pricing_infinite = ModelPricing {
        cache_read_input_token_cost: Some(0.0000001),
        cache_read_input_token_cost_above_200k_tokens: Some(f64::INFINITY),
        ..Default::default()
    };
    let pricing_nan = ModelPricing {
        cache_read_input_token_cost: Some(0.0000001),
        cache_read_input_token_cost_above_200k_tokens: Some(f64::NAN),
        ..Default::default()
    };

    let expected = 200_001.0 * 0.0000001;
    assert!((compute_cost(&pricing_negative, 0, 0, 200_001, 0, 0) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_infinite, 0, 0, 200_001, 0, 0) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_nan, 0, 0, 200_001, 0, 0) - expected).abs() < 1e-12);
}

#[test]
fn test_compute_cost_tiered_invalid_above_rate_falls_back_to_base_cache_write() {
    let pricing_negative = ModelPricing {
        cache_creation_input_token_cost: Some(0.0000003),
        cache_creation_input_token_cost_above_200k_tokens: Some(-0.0000004),
        ..Default::default()
    };
    let pricing_infinite = ModelPricing {
        cache_creation_input_token_cost: Some(0.0000003),
        cache_creation_input_token_cost_above_200k_tokens: Some(f64::INFINITY),
        ..Default::default()
    };
    let pricing_nan = ModelPricing {
        cache_creation_input_token_cost: Some(0.0000003),
        cache_creation_input_token_cost_above_200k_tokens: Some(f64::NAN),
        ..Default::default()
    };

    let expected = 200_001.0 * 0.0000003;
    assert!((compute_cost(&pricing_negative, 0, 0, 0, 200_001, 0) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_infinite, 0, 0, 0, 200_001, 0) - expected).abs() < 1e-12);
    assert!((compute_cost(&pricing_nan, 0, 0, 0, 200_001, 0) - expected).abs() < 1e-12);
}

#[test]
fn test_calculate_cost_tiered_all_buckets_with_reasoning_threshold_crossing() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000001),
            input_cost_per_token_above_200k_tokens: Some(0.000002),
            output_cost_per_token: Some(0.000003),
            output_cost_per_token_above_200k_tokens: Some(0.000004),
            cache_read_input_token_cost: Some(0.0000001),
            cache_read_input_token_cost_above_200k_tokens: Some(0.0000002),
            cache_creation_input_token_cost: Some(0.0000003),
            cache_creation_input_token_cost_above_200k_tokens: Some(0.0000004),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let cost = lookup.calculate_cost("claude-opus-4-6", 200_001, 199_999, 200_001, 200_001, 2);

    let expected_input = 200_000.0 * 0.000001 + 0.000002;
    let expected_output = 200_000.0 * 0.000003 + 0.000004; // output + reasoning = 200_001
    let expected_cache_read = 200_000.0 * 0.0000001 + 0.0000002;
    let expected_cache_write = 200_000.0 * 0.0000003 + 0.0000004;
    let expected = expected_input + expected_output + expected_cache_read + expected_cache_write;

    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn test_calculate_cost_unknown_model() {
    let lookup = create_lookup();
    let cost = lookup.calculate_cost("nonexistent-model", 1_000_000, 500_000, 0, 0, 0);
    assert_eq!(cost, 0.0);
}
