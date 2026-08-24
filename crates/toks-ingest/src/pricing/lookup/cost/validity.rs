use super::super::LookupResult;
use crate::pricing::litellm::ModelPricing;

pub(in crate::pricing::lookup) fn is_valid_price_value(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

/// Returns true if the pricing entry has at least one usable cost field
/// (base or above-200k tier). Entries with all-None pricing (e.g.
/// subscription-based providers like Perplexity) are useless for
/// pay-per-token cost estimation and should be deprioritized.
pub(in crate::pricing::lookup) fn has_any_usable_pricing(pricing: &ModelPricing) -> bool {
    pricing
        .all_rates()
        .into_iter()
        .any(|opt| opt.is_some_and(is_valid_price_value))
}

pub(in crate::pricing::lookup) fn lookup_result_if_usable(
    pricing: &ModelPricing,
    source: &str,
    matched_key: &str,
) -> Option<LookupResult> {
    has_any_usable_pricing(pricing).then(|| LookupResult {
        pricing: pricing.clone(),
        source: source.into(),
        matched_key: matched_key.into(),
    })
}

pub(in crate::pricing::lookup) fn has_any_valid_above_tier_value(pricing: &ModelPricing) -> bool {
    [
        pricing.input_cost_per_token_above_128k_tokens,
        pricing.input_cost_per_token_above_200k_tokens,
        pricing.input_cost_per_token_above_256k_tokens,
        pricing.input_cost_per_token_above_272k_tokens,
        pricing.output_cost_per_token_above_128k_tokens,
        pricing.output_cost_per_token_above_200k_tokens,
        pricing.output_cost_per_token_above_256k_tokens,
        pricing.output_cost_per_token_above_272k_tokens,
        pricing.cache_read_input_token_cost_above_200k_tokens,
        pricing.cache_read_input_token_cost_above_272k_tokens,
        pricing.cache_creation_input_token_cost_above_200k_tokens,
    ]
    .into_iter()
    .flatten()
    .any(is_valid_price_value)
}

pub(in crate::pricing::lookup) fn has_meaningful_tier_support(pricing: &ModelPricing) -> bool {
    [
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_128k_tokens,
        ),
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_200k_tokens,
        ),
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_256k_tokens,
        ),
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_272k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_128k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_200k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_256k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_272k_tokens,
        ),
    ]
    .into_iter()
    .any(|(base, above)| match (base, above) {
        (Some(base), Some(above)) => base.is_finite() && base >= 0.0 && is_valid_price_value(above),
        _ => false,
    })
}
