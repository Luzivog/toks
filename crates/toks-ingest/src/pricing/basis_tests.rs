use super::{compute_basis_cost, PricingBasis};
use crate::pricing::lookup::compute_cost;
use crate::pricing::ModelPricing;
use crate::TokenBreakdown;
use std::collections::HashMap;

#[test]
fn aggregated_requests_keep_their_original_tier_bands() {
    let pricing = ModelPricing {
        input_cost_per_token: Some(1.0),
        input_cost_per_token_above_200k_tokens: Some(2.0),
        output_cost_per_token: Some(0.0),
        ..Default::default()
    };
    let request = TokenBreakdown {
        input: 200_000,
        ..Default::default()
    };
    let mut aggregate = PricingBasis::from_usage(&request);
    aggregate.add_assign(PricingBasis::from_usage(&request));

    assert_eq!(compute_basis_cost(&pricing, &aggregate), 400_000.0);
    assert_eq!(compute_cost(&pricing, 400_000, 0, 0, 0, 0), 600_000.0);
}

#[test]
fn cache_write_and_reasoning_remain_separate_billable_buckets() {
    let pricing = ModelPricing {
        output_cost_per_token: Some(3.0),
        cache_creation_input_token_cost: Some(4.0),
        ..Default::default()
    };
    let basis = PricingBasis::from_usage(&TokenBreakdown {
        output: 7,
        reasoning: 5,
        cache_write: 11,
        ..Default::default()
    });

    assert_eq!(compute_basis_cost(&pricing, &basis), 80.0);
}

#[test]
fn openai_long_context_class_is_preserved_per_request() {
    let pricing = ModelPricing {
        input_cost_per_token: Some(5e-6),
        input_cost_per_token_above_272k_tokens: Some(10e-6),
        output_cost_per_token: Some(30e-6),
        output_cost_per_token_above_272k_tokens: Some(45e-6),
        cache_read_input_token_cost: Some(0.5e-6),
        cache_read_input_token_cost_above_272k_tokens: Some(1e-6),
        ..Default::default()
    };
    let service = crate::pricing::PricingService::new(
        HashMap::from([("gpt-5.6-sol".into(), pricing)]),
        HashMap::new(),
    );
    let request = TokenBreakdown {
        input: 200_000,
        ..Default::default()
    };
    let mut basis = PricingBasis::from_usage(&request);
    basis.add_assign(PricingBasis::from_usage(&request));
    let aggregate = TokenBreakdown {
        input: 400_000,
        ..Default::default()
    };

    let request_correct = service.calculate_basis_cost_with_provider(
        "gpt-5.6-sol",
        Some("openai"),
        &aggregate,
        &basis,
        false,
    );
    let aggregated_as_one_request =
        service.calculate_cost_with_provider("gpt-5.6-sol", Some("openai"), &aggregate);

    assert!((request_correct - 2.0).abs() < 1e-12);
    assert!((aggregated_as_one_request - 4.0).abs() < 1e-12);
}
