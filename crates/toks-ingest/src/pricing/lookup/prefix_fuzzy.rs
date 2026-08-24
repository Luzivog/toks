mod fuzzy;
mod prefix;
mod strip;

use super::normalize::normalize_version_separator;
use super::select::choose_best_source_result;
use super::{LookupResult, PricingLookup};

pub(in crate::pricing::lookup) use strip::{try_strip_unknown_prefix, try_strip_unknown_suffix};

// Bare brand tokens ("claude", "anthropic", "gemini") are blocked because they
// contain no model information: a fuzzy hit from them can land on any model of
// the brand (e.g. retired `claude-2.1` eroding to `claude` and billing at an
// opus-fast key, or `gemini-default` eroding to `gemini` and landing on a
// native-audio preview key), so such a match is never trustworthy.
//
// Generic English words ("model", "router", "default") are blocked for the same
// reason: they carry no model identity, yet substring-match real priced keys
// (`azure_ai/model_router`, `kilo/switchpoint/router`, `fireworks-ai-default`).
// Without this guard an id whose only fuzzy-eligible remnant after suffix
// stripping is the word `model` (e.g. `model-zero-usage-v1` -> stripped
// `model`) misprices at the router key's rate. See
// `fuzzy_match_does_not_resolve_generic_model_token`.
//
// `default` is the same failure with a live victim: the generic routing label
// `gemini-default` strips to `default`, which fuzzy-hits LiteLLM's real
// `fireworks-ai-default` row. That row prices at 0.0/0.0, and
// `ModelPricing::covers_usage` treats an explicit zero as a real rate, so the
// label looked *priced* — enough to slip past
// `exclude_unpriced_submission_messages` and be submitted at
// Fireworks AI's rates. A Google routing label is not a Fireworks model.
// See `fuzzy_match_does_not_resolve_generic_default_token`.
pub(in crate::pricing::lookup) const FUZZY_BLOCKLIST: &[&str] = &[
    "auto",
    "mini",
    "chat",
    "base",
    "claude",
    "anthropic",
    "gemini",
    "model",
    "router",
    "default",
];

const MIN_FUZZY_MATCH_LEN: usize = 5;

pub(in crate::pricing::lookup) fn lookup_prefix(
    lookup: &PricingLookup,
    model_id: &str,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    if let Some(result) = lookup.prefix_match_litellm(model_id, provider_id) {
        return Some(result);
    }
    if let Some(result) = lookup.prefix_match_openrouter(model_id, provider_id) {
        return Some(result);
    }
    if let Some(result) = lookup.prefix_match_models_dev(model_id, provider_id) {
        return Some(result);
    }

    if let Some(version_normalized) = normalize_version_separator(model_id) {
        if let Some(result) = lookup.prefix_match_litellm(&version_normalized, provider_id) {
            return Some(result);
        }
        if let Some(result) = lookup.prefix_match_openrouter(&version_normalized, provider_id) {
            return Some(result);
        }
        if let Some(result) = lookup.prefix_match_models_dev(&version_normalized, provider_id) {
            return Some(result);
        }
    }

    None
}

pub(in crate::pricing::lookup) fn lookup_fuzzy(
    lookup: &PricingLookup,
    model_id: &str,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    if !is_fuzzy_eligible(model_id) {
        return None;
    }

    choose_best_source_result(
        lookup.fuzzy_match_litellm(model_id, provider_id),
        lookup.fuzzy_match_openrouter(model_id, provider_id),
        provider_id,
    )
}

pub(in crate::pricing::lookup) fn is_fuzzy_eligible(model_id: &str) -> bool {
    if model_id.len() < MIN_FUZZY_MATCH_LEN {
        return false;
    }
    !FUZZY_BLOCKLIST.contains(&model_id)
}
