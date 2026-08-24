use super::super::cost::lookup_result_if_usable;
use super::super::prefix_fuzzy::FUZZY_BLOCKLIST;
use super::super::{LookupResult, PricingLookup};

impl PricingLookup {
    pub(in crate::pricing::lookup) fn exact_match_litellm_for_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_with_provider_prefixes(
            model_id,
            provider_id,
            &self.litellm_key_parts,
            &self.litellm,
            "LiteLLM",
        )
    }

    pub(in crate::pricing::lookup) fn exact_match_openrouter_for_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_with_provider_prefixes(
            model_id,
            provider_id,
            &self.openrouter_key_parts,
            &self.openrouter,
            "OpenRouter",
        )
    }

    pub(in crate::pricing::lookup) fn exact_match_openrouter_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_openrouter_for_provider(model_id, provider_id)
            .or_else(|| self.exact_match_openrouter(model_id))
    }

    pub(in crate::pricing::lookup) fn exact_match_models_dev_for_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_with_provider_prefixes(
            model_id,
            provider_id,
            &self.models_dev_key_parts,
            &self.models_dev,
            "Models.dev",
        )
    }

    pub(in crate::pricing::lookup) fn exact_match_models_dev_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_models_dev_for_provider(model_id, provider_id)
            .or_else(|| self.exact_match_models_dev(model_id))
    }

    pub(in crate::pricing::lookup) fn exact_match_litellm(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        let key = self.litellm_lower.get(model_id)?;
        let pricing = self.litellm.get(key)?;
        lookup_result_if_usable(pricing, "LiteLLM", key)
    }

    pub(in crate::pricing::lookup) fn exact_match_openrouter(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        self.exact_match_openrouter_full_key(model_id)
            .or_else(|| self.exact_match_openrouter_model_part(model_id))
    }

    /// Full-key (`provider/model`) exact match against OpenRouter — the id's
    /// own canonical key. This wins even under a provider hint.
    pub(in crate::pricing::lookup) fn exact_match_openrouter_full_key(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        let key = self.openrouter_lower.get(model_id)?;
        let pricing = self.openrouter.get(key)?;
        lookup_result_if_usable(pricing, "OpenRouter", key)
    }

    /// Model-part exact match against OpenRouter — matches any provider whose
    /// model-part equals `model_id`. A provider hint must take precedence over
    /// this (see `lookup_auto`), otherwise a hinted lookup leaks to a different
    /// provider's canonical key.
    ///
    /// The model-part index is a cross-provider fallback in the same trust
    /// class as fuzzy matching: it lands the id on "some other provider's
    /// model whose model-part equals this id". Generic tokens on the
    /// `FUZZY_BLOCKLIST` carry no model identity, and #1070's resolver-top
    /// `is_routing_label` guard already refuses the router labels it knows
    /// (`auto`, `agent_review`). This blocklist gate is the second layer:
    /// it covers generic tokens no parser emits today but any provider could
    /// publish as a model part tomorrow (`default`, `router`, `mini`, ...),
    /// and it protects any path that reaches the model-part index without
    /// passing through that guard. Full-key matches, which are the id's own
    /// canonical key, stay honored.
    pub(in crate::pricing::lookup) fn exact_match_openrouter_model_part(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        if FUZZY_BLOCKLIST.contains(&model_id) {
            return None;
        }
        let key = self.openrouter_model_part.get(model_id)?;
        let pricing = self.openrouter.get(key)?;
        lookup_result_if_usable(pricing, "OpenRouter", key)
    }

    pub(in crate::pricing::lookup) fn exact_match_models_dev(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        if let Some(key) = self.models_dev_lower.get(model_id) {
            if let Some(pricing) = self.models_dev.get(key) {
                return Some(LookupResult {
                    pricing: pricing.clone(),
                    source: "Models.dev".into(),
                    matched_key: key.clone(),
                });
            }
        }
        // Same cross-provider fallback trust class as the OpenRouter model-part
        // index: #1070's resolver-top guard plus this blocklist gate keep bare
        // generic tokens off another provider's model part, while the id's own
        // full dataset key (`morph/auto`) still resolves.
        if !FUZZY_BLOCKLIST.contains(&model_id) {
            if let Some(key) = self.models_dev_model_part.get(model_id) {
                if let Some(pricing) = self.models_dev.get(key) {
                    return Some(LookupResult {
                        pricing: pricing.clone(),
                        source: "Models.dev".into(),
                        matched_key: key.clone(),
                    });
                }
            }
        }
        None
    }
}
