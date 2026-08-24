use super::super::cost::lookup_result_if_usable;
use super::super::provider::PROVIDER_PREFIXES;
use super::super::{LookupResult, PricingLookup};

impl PricingLookup {
    pub(in crate::pricing::lookup) fn prefix_match_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_litellm_for_provider(model_id, provider_id) {
            return Some(result);
        }

        for prefix in PROVIDER_PREFIXES {
            let key = format!("{}{}", prefix, model_id);
            if let Some(litellm_key) = self.litellm_lower.get(&key) {
                if let Some(pricing) = self.litellm.get(litellm_key) {
                    if let Some(result) = lookup_result_if_usable(pricing, "LiteLLM", litellm_key) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    pub(in crate::pricing::lookup) fn prefix_match_openrouter(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_openrouter_for_provider(model_id, provider_id) {
            return Some(result);
        }

        for prefix in PROVIDER_PREFIXES {
            let key = format!("{}{}", prefix, model_id);
            if let Some(or_key) = self.openrouter_lower.get(&key) {
                if let Some(pricing) = self.openrouter.get(or_key) {
                    if let Some(result) = lookup_result_if_usable(pricing, "OpenRouter", or_key) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    pub(in crate::pricing::lookup) fn prefix_match_models_dev(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_models_dev_for_provider(model_id, provider_id) {
            return Some(result);
        }

        for prefix in PROVIDER_PREFIXES {
            let key = format!("{}{}", prefix, model_id);
            if let Some(models_dev_key) = self.models_dev_lower.get(&key) {
                if let Some(pricing) = self.models_dev.get(models_dev_key) {
                    return Some(LookupResult {
                        pricing: pricing.clone(),
                        source: "Models.dev".into(),
                        matched_key: models_dev_key.clone(),
                    });
                }
            }
        }
        None
    }
}
