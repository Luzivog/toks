use super::super::LookupResult;
use super::openai::uses_openai_full_request_272k_pricing;
use super::validity::is_valid_price_value;
use crate::pricing::{
    basis::{compute_basis_cost, PricingBasis},
    litellm::ModelPricing,
};
use crate::TokenBreakdown;

const TIERED_PRICING_THRESHOLD_272K_TOKENS: f64 = 272_000.0;

pub(in crate::pricing::lookup) fn compute_cost_for_lookup(
    result: &LookupResult,
    provider_id: Option<&str>,
    usage: &TokenBreakdown,
) -> f64 {
    let total_input = usage
        .input
        .max(0)
        .saturating_add(usage.cache_read.max(0))
        .saturating_add(usage.cache_write.max(0));
    compute_basis_cost_for_lookup(
        result,
        provider_id,
        &PricingBasis::from_usage(usage),
        total_input > TIERED_PRICING_THRESHOLD_272K_TOKENS as i64,
    )
}

pub(in crate::pricing::lookup) fn compute_basis_cost_for_lookup(
    result: &LookupResult,
    provider_id: Option<&str>,
    basis: &PricingBasis,
    long_context: bool,
) -> f64 {
    if !uses_openai_full_request_272k_pricing(result, provider_id) {
        return compute_basis_cost(&result.pricing, basis);
    }

    let mut pricing = result.pricing.clone();
    if !long_context {
        pricing.input_cost_per_token_above_272k_tokens = None;
        pricing.output_cost_per_token_above_272k_tokens = None;
        pricing.cache_read_input_token_cost_above_272k_tokens = None;
        return compute_basis_cost(&pricing, basis);
    }

    if let Some(high) = pricing
        .input_cost_per_token_above_272k_tokens
        .filter(|price| is_valid_price_value(*price))
    {
        let input_multiplier = pricing
            .input_cost_per_token
            .filter(|base| is_valid_price_value(*base) && *base > 0.0)
            .map(|base| high / base);
        for rate in [
            &mut pricing.input_cost_per_token,
            &mut pricing.input_cost_per_token_above_128k_tokens,
            &mut pricing.input_cost_per_token_above_200k_tokens,
            &mut pricing.input_cost_per_token_above_256k_tokens,
            &mut pricing.input_cost_per_token_above_272k_tokens,
        ] {
            *rate = Some(high);
        }

        if let (Some(multiplier), Some(cache_write_price)) = (
            input_multiplier,
            pricing
                .cache_creation_input_token_cost
                .filter(|price| is_valid_price_value(*price)),
        ) {
            let high = Some(cache_write_price * multiplier);
            pricing.cache_creation_input_token_cost = high;
            pricing.cache_creation_input_token_cost_above_200k_tokens = high;
        }
    }
    if let Some(high) = pricing
        .output_cost_per_token_above_272k_tokens
        .filter(|price| is_valid_price_value(*price))
    {
        for rate in [
            &mut pricing.output_cost_per_token,
            &mut pricing.output_cost_per_token_above_128k_tokens,
            &mut pricing.output_cost_per_token_above_200k_tokens,
            &mut pricing.output_cost_per_token_above_256k_tokens,
            &mut pricing.output_cost_per_token_above_272k_tokens,
        ] {
            *rate = Some(high);
        }
    }
    if let Some(high) = pricing
        .cache_read_input_token_cost_above_272k_tokens
        .filter(|price| is_valid_price_value(*price))
    {
        for rate in [
            &mut pricing.cache_read_input_token_cost,
            &mut pricing.cache_read_input_token_cost_above_200k_tokens,
            &mut pricing.cache_read_input_token_cost_above_272k_tokens,
        ] {
            *rate = Some(high);
        }
    }

    compute_basis_cost(&pricing, basis)
}

pub fn compute_cost(
    pricing: &ModelPricing,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
) -> f64 {
    compute_basis_cost(
        pricing,
        &PricingBasis::from_usage(&TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            reasoning,
        }),
    )
}
