use super::super::cost::{has_any_valid_above_tier_value, has_meaningful_tier_support};
use super::super::select::choose_best_source_result_with_models_dev;
use super::super::{LookupResult, PricingLookup};
use super::hints::model_prefix_matches_provider;
use super::prefixes::strip_known_provider_prefix;

pub(in crate::pricing::lookup) fn lookup_known_provider_prefix(
    lookup: &PricingLookup,
    model_id: &str,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    let stripped = strip_known_provider_prefix(model_id)?;
    let prefix_matches_hint =
        provider_id.is_none() || model_prefix_matches_provider(model_id, provider_id);

    if prefix_matches_hint {
        if let Some(exact_litellm) = lookup.exact_match_litellm(model_id) {
            return Some(exact_litellm);
        }

        let exact_openrouter = lookup.exact_match_openrouter(model_id);
        let stripped_litellm = lookup.exact_or_normalized_litellm(stripped, provider_id);

        if let (Some(litellm), Some(openrouter)) = (&stripped_litellm, &exact_openrouter) {
            if has_meaningful_tier_support(&litellm.pricing)
                && !has_any_valid_above_tier_value(&openrouter.pricing)
            {
                return stripped_litellm;
            }
        }

        if let Some(result) = exact_openrouter {
            return Some(result);
        }
        if let Some(result) = stripped_litellm {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_models_dev(model_id) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_models_dev_with_provider(stripped, provider_id) {
            return Some(result);
        }
    } else {
        if let Some(result) = choose_best_source_result_with_models_dev(
            lookup.exact_match_litellm_for_provider(stripped, provider_id),
            lookup.exact_match_openrouter_for_provider(stripped, provider_id),
            lookup.exact_match_models_dev_for_provider(stripped, provider_id),
            provider_id,
        ) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_or_normalized_litellm(stripped, provider_id) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_models_dev_with_provider(stripped, provider_id) {
            return Some(result);
        }
    }

    None
}
