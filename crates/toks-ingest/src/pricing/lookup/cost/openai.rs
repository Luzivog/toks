use super::super::{select::is_reseller_provider, LookupResult};
use super::validity::is_valid_price_value;
use crate::pricing::litellm::ModelPricing;
use crate::provider_identity;

fn matches_model_or_snapshot(model_id: &str, base: &str) -> bool {
    model_id == base
        || model_id
            .strip_prefix(base)
            .is_some_and(|suffix| suffix.starts_with("-20"))
}

fn is_openai_full_request_272k_model(model_id: &str) -> bool {
    let key = model_id.to_ascii_lowercase();
    let model_id = key.split('/').next_back().unwrap_or(&key);

    [
        "gpt-5.4",
        "gpt-5.4-pro",
        "gpt-5.5",
        // Priced identically to gpt-5.4-pro in LiteLLM ($30/$180 base,
        // $60/$270 above 272k) with the same full-request semantics.
        "gpt-5.5-pro",
        "gpt-5.6",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-5.6-luna",
    ]
    .into_iter()
    .any(|base| matches_model_or_snapshot(model_id, base))
}

pub(in crate::pricing::lookup) fn should_prefer_openai_tiered_litellm(
    model_id: &str,
    provider_id: Option<&str>,
    litellm: Option<&LookupResult>,
) -> bool {
    provider_id.is_some_and(|provider| {
        provider_identity::canonical_provider(provider).as_deref() == Some("openai")
    }) && is_openai_full_request_272k_model(model_id)
        && litellm.is_some_and(|result| has_complete_openai_272k_pricing(&result.pricing))
}

// A fully-absent cache_read pair used to count as "complete" here (only a
// present-but-partial pair failed), which let the 272k LiteLLM preference
// fire over an OpenRouter entry that actually had cache-read pricing,
// silently dropping it. cache_read is now required present+valid like
// input/output, symmetric with them, for this preference decision only.
pub(in crate::pricing::lookup) fn has_complete_openai_272k_pricing(pricing: &ModelPricing) -> bool {
    let valid_pair = |base: Option<f64>, above: Option<f64>| {
        base.is_some_and(is_valid_price_value) && above.is_some_and(is_valid_price_value)
    };

    valid_pair(
        pricing.input_cost_per_token,
        pricing.input_cost_per_token_above_272k_tokens,
    ) && valid_pair(
        pricing.output_cost_per_token,
        pricing.output_cost_per_token_above_272k_tokens,
    ) && valid_pair(
        pricing.cache_read_input_token_cost,
        pricing.cache_read_input_token_cost_above_272k_tokens,
    )
}

pub(in crate::pricing::lookup) fn uses_openai_full_request_272k_pricing(
    result: &LookupResult,
    provider_id: Option<&str>,
) -> bool {
    if result.source != "LiteLLM"
        || is_reseller_provider(&result.matched_key)
        || provider_id.is_some_and(|provider| {
            provider_identity::canonical_provider(provider).as_deref() != Some("openai")
        })
    {
        return false;
    }

    let key = result.matched_key.to_ascii_lowercase();
    if key.contains('/') && !key.starts_with("openai/") {
        return false;
    }

    is_openai_full_request_272k_model(&key)
}
