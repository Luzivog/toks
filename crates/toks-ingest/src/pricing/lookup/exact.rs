mod matches;
mod overrides;
mod provider;

use super::cost::should_prefer_openai_tiered_litellm;
use super::normalize::{normalize_model_name, normalize_version_separator};
use super::select::choose_best_source_result_with_models_dev;
use super::{LookupResult, PricingLookup};

pub(in crate::pricing::lookup) fn lookup(
    lookup: &PricingLookup,
    model_id: &str,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    let exact_litellm = lookup.exact_match_litellm(model_id);
    if should_prefer_openai_tiered_litellm(model_id, provider_id, exact_litellm.as_ref()) {
        return exact_litellm;
    }

    if let Some(result) = choose_best_source_result_with_models_dev(
        lookup.exact_match_litellm_for_provider(model_id, provider_id),
        lookup.exact_match_openrouter_for_provider(model_id, provider_id),
        lookup.exact_match_models_dev_for_provider(model_id, provider_id),
        provider_id,
    ) {
        return Some(result);
    }

    if let Some(result) = exact_litellm {
        return Some(result);
    }
    // An unscoped OpenRouter FULL-KEY match is the id's own canonical key,
    // so it wins even under a provider hint. The MODEL-PART fallback does
    // not: it matches "some other provider's model whose model-part equals
    // this id", which is exactly what a provider hint must override.
    if let Some(result) = lookup.exact_match_openrouter_full_key(model_id) {
        return Some(result);
    }

    // A provider hint pins the lookup to that provider's catalog: the
    // provider-scoped models.dev pass must run before BOTH the unscoped
    // OpenRouter model-part fallback here and the separator-normalized
    // fallback below. Otherwise a hinted lookup (e.g. `venice` + dotted
    // `claude-opus-4.6-fast`, which already matches OpenRouter's
    // `anthropic/claude-opus-4.6-fast` model-part) would take the canonical
    // price instead of the hinted provider's own key. A hint with no
    // matching key falls through to the canonical resolution below.
    if provider_id.is_some() {
        if let Some(result) = lookup.exact_match_models_dev_for_provider(model_id, provider_id) {
            return Some(result);
        }
    }
    if let Some(result) = lookup.exact_match_openrouter_model_part(model_id) {
        return Some(result);
    }

    // Separator-normalized exact passes against the canonical sources
    // (LiteLLM + OpenRouter) run BEFORE the models.dev model-part pass so
    // ids like `claude-opus-4-6-fast` hit the canonical
    // `anthropic/claude-opus-4.6-fast` key instead of a reseller's
    // `venice/claude-opus-4-6-fast` markup. models.dev stays the
    // long-tail fallback below. This reorder only preempts models.dev
    // for UNhinted lookups: the provider-scoped passes above and below
    // keep provider-hinted resolutions pinned to the hinted provider.
    if let Some(version_normalized) = normalize_version_separator(model_id) {
        if let Some(result) = choose_best_source_result_with_models_dev(
            lookup.exact_match_litellm_for_provider(&version_normalized, provider_id),
            lookup.exact_match_openrouter_for_provider(&version_normalized, provider_id),
            lookup.exact_match_models_dev_for_provider(&version_normalized, provider_id),
            provider_id,
        ) {
            return Some(result);
        }
        if provider_id.is_some() {
            if let Some(result) =
                lookup.exact_match_models_dev_for_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(result) = lookup.exact_match_litellm(&version_normalized) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_openrouter(&version_normalized) {
            return Some(result);
        }
    }

    if let Some(result) = lookup.exact_match_models_dev_with_provider(model_id, provider_id) {
        return Some(result);
    }
    if let Some(version_normalized) = normalize_version_separator(model_id) {
        if let Some(result) =
            lookup.exact_match_models_dev_with_provider(&version_normalized, provider_id)
        {
            return Some(result);
        }
    }

    if let Some(normalized) = normalize_model_name(model_id) {
        if let Some(result) = choose_best_source_result_with_models_dev(
            lookup.exact_match_litellm_for_provider(&normalized, provider_id),
            lookup.exact_match_openrouter_for_provider(&normalized, provider_id),
            lookup.exact_match_models_dev_for_provider(&normalized, provider_id),
            provider_id,
        ) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_litellm(&normalized) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_openrouter(&normalized) {
            return Some(result);
        }
        if let Some(result) = lookup.exact_match_models_dev_with_provider(&normalized, provider_id)
        {
            return Some(result);
        }
    }

    None
}

pub(in crate::pricing::lookup) fn lookup_overrides(
    lookup: &PricingLookup,
    model_id: &str,
) -> Option<LookupResult> {
    if let Some(result) = lookup.exact_match_cursor(model_id) {
        return Some(result);
    }
    if let Some(version_normalized) = normalize_version_separator(model_id) {
        if let Some(result) = lookup.exact_match_cursor(&version_normalized) {
            return Some(result);
        }
    }

    // Sakana built-in overrides sit at the SAME precedence as Cursor:
    // upstream real prices (litellm/openrouter/models.dev exact + prefix)
    // already won above, so Sakana only catches ids upstream doesn't price,
    // while still beating the fuzzy guesses below.
    if let Some(result) = lookup.exact_match_sakana(model_id) {
        return Some(result);
    }
    if let Some(version_normalized) = normalize_version_separator(model_id) {
        if let Some(result) = lookup.exact_match_sakana(&version_normalized) {
            return Some(result);
        }
    }

    None
}
