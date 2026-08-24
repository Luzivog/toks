use super::super::select::select_best_match;
use super::super::state::{KeyModelPart, LookupResult, PricingLookup};
use crate::pricing::litellm::ModelPricing;
use crate::provider_identity;
use std::collections::HashMap;

impl PricingLookup {
    pub(in crate::pricing::lookup) fn exact_match_with_provider_prefixes(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        key_parts: &[KeyModelPart],
        dataset: &HashMap<String, ModelPricing>,
        source: &str,
    ) -> Option<LookupResult> {
        let provider_id = provider_id?;
        let hint_tags = provider_identity::provider_tags(provider_id);

        let matches: Vec<&String> = key_parts
            .iter()
            .filter(|kp| {
                model_part_matches_exact(&kp.lower_model_part, model_id)
                    && provider_identity::matches_provider_hint_with_tags(&kp.key, &hint_tags)
            })
            .map(|kp| &kp.key)
            .collect();

        if matches.is_empty() {
            return None;
        }

        select_best_match(&matches, dataset, source, Some(provider_id))
    }
}

fn model_part_matches_exact(model_part: &str, model_id: &str) -> bool {
    if model_part == model_id {
        return true;
    }

    let mut suffix = model_part;
    while let Some((_, rest)) = suffix.split_once('.') {
        if rest == model_id {
            return true;
        }
        suffix = rest;
    }

    false
}
