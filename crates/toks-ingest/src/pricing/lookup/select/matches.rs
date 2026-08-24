use super::provider_rank::{
    is_original_provider, is_reseller_provider, key_root_is_cross_provider_alias,
    key_root_matches_hint,
};
use crate::pricing::litellm::ModelPricing;
use crate::pricing::lookup::cost::has_any_usable_pricing;
use crate::pricing::lookup::LookupResult;
use crate::provider_identity;
use std::collections::HashMap;

pub(in crate::pricing::lookup) fn select_best_match(
    matches: &[&String],
    dataset: &HashMap<String, ModelPricing>,
    source: &str,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    if matches.is_empty() {
        return None;
    }

    let hint_tags: Vec<String> = provider_id
        .map(provider_identity::provider_tags)
        .unwrap_or_default();

    let provider_matches: Vec<&String> = matches
        .iter()
        .copied()
        .filter(|key| provider_identity::matches_provider_hint_with_tags(key, &hint_tags))
        .collect();

    let preferred_matches = if provider_matches.is_empty() {
        matches
    } else {
        provider_matches.as_slice()
    };

    // Deprioritize entries with all-None pricing (e.g. perplexity/anthropic/...
    // which matches provider hint "anthropic" but has subscription-based pricing
    // with no per-token cost data). If provider-specific candidates are all
    // unusable, fall back to any priced candidate in the broader match set so
    // fuzzy/provider-aware lookups can still resolve a valid non-provider key.
    let preferred_with_pricing: Vec<&String> = preferred_matches
        .iter()
        .copied()
        .filter(|k| dataset.get(k.as_str()).is_some_and(has_any_usable_pricing))
        .collect();
    let effective_matches: Vec<&String> =
        if preferred_with_pricing.is_empty() && !provider_matches.is_empty() {
            matches
                .iter()
                .copied()
                .filter(|k| dataset.get(k.as_str()).is_some_and(has_any_usable_pricing))
                .collect()
        } else {
            preferred_with_pricing
        };
    if effective_matches.is_empty() {
        return None;
    }

    let hint_is_reseller = provider_id.is_some_and(is_reseller_provider);
    let key = pick_key(
        effective_matches.as_slice(),
        hint_is_reseller,
        &hint_tags,
        provider_id,
    )?;
    dataset.get(key.as_str()).map(|pricing| LookupResult {
        pricing: pricing.clone(),
        source: source.into(),
        matched_key: key.clone(),
    })
}

fn pick_key<'a>(
    candidates: &'a [&'a String],
    prefer_reseller: bool,
    hint_tags: &[String],
    provider_id: Option<&str>,
) -> Option<&'a String> {
    if prefer_reseller {
        return candidates
            .iter()
            .copied()
            .find(|k| is_reseller_provider(k))
            .or_else(|| candidates.first().copied());
    }

    // The vendor-spelling fold (`deepseek-ai` -> `deepseek`) widens
    // this pool: a `deepseek` hint now matches both
    // `novita/deepseek/<model>` and `cloudflare/@cf/deepseek-ai/<model>`,
    // two resellers with different price sheets for the same weights.
    // Nothing below tells them apart, so the winner falls out of key
    // ordering — which is length-descending over a HashMap's key
    // iteration, and therefore not even stable between processes for
    // equal-length keys. `deepseek-r1-distill-qwen-32b` with the
    // `deepseek` hint that `inferred_provider_from_model` synthesizes
    // moved off `novita/deepseek/...` at $0.30/$0.30 per MTok onto
    // `cloudflare/@cf/deepseek-ai/...` at $0.497/$4.881 — a 16x output
    // rate on the same weights.
    //
    // So the pool is ranked explicitly instead of leaning on key
    // order. The hinted vendor's own top-level row wins first:
    // `novita-ai/moonshotai/kimi-k2.6` at $0.80/$3.40 is Novita's own,
    // while `poe/novita/kimi-k2.6` at $0.96/$4.04 spells `novita` in a
    // nested segment only because Poe is reselling it. Ranking that row
    // rather than merely detecting it matters, because candidates are
    // ordered longest key first and the vendor's own row is usually the
    // shorter one: `vercel_ai_gateway/zai/glm-4.6` at $0.45/$1.80 would
    // otherwise be billed for a `zai` hint that Z.ai itself publishes
    // at `zai/glm-4.6`, $0.60/$2.20. A raw Vertex hint similarly keeps
    // Vertex's hosted row ahead of Anthropic's row, while an Anthropic
    // hint excludes that cross-provider root alias. A first-party row
    // is the next tier.
    //
    // Then comes a row that spells the vendor exactly as the hint does,
    // in preference to one that only matches after folding. That row is
    // taken even when it starts with a reseller prefix, because the
    // property that matters is the spelling, not the publisher: the
    // pre-fold match for a `deepseek-ai` hint on `deepseek-r1` is
    // `together_ai/deepseek-ai/DeepSeek-R1` at $3.00/$7.00, and
    // discarding it for being a reseller just hands the lookup to
    // `vercel_ai_gateway/deepseek/deepseek-r1` at $0.55/$2.19 — another
    // reseller, chosen for being spelled the other way and having a
    // longer key. Among equally spelled rows a non-reseller still wins.
    let by_root = candidates.iter().copied().find(|k| {
        key_root_matches_hint(k, hint_tags)
            && !provider_id.is_some_and(|hint| key_root_is_cross_provider_alias(k, hint))
    });
    let by_spelling = provider_id.and_then(|hint| {
        let spelled: Vec<&String> = candidates
            .iter()
            .copied()
            .filter(|k| provider_identity::matches_provider_spelling(k, hint))
            .collect();
        spelled
            .iter()
            .copied()
            .find(|k| !is_reseller_provider(k))
            .or_else(|| spelled.first().copied())
    });
    by_root
        .or_else(|| candidates.iter().copied().find(|k| is_original_provider(k)))
        .or(by_spelling)
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .find(|k| !is_reseller_provider(k))
        })
        .or_else(|| candidates.first().copied())
}
