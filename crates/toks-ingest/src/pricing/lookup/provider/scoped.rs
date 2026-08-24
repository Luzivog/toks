use super::super::cost::lookup_result_if_usable;
use super::super::select::choose_best_source_result;
use super::super::state::ProviderScopedModelPath;
use super::super::{LookupResult, PricingLookup};
use super::hints::{
    provider_hint_matches_scoped_provider, provider_prefix_matches_scoped_provider,
};
use super::RESELLER_PROVIDER_PREFIXES;
use crate::provider_identity;

impl PricingLookup {
    pub(in crate::pricing::lookup) fn lookup_provider_scoped_path(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let scoped = parse_provider_scoped_model_path(model_id)?;
        if !provider_hint_matches_scoped_provider(provider_id, scoped.provider) {
            return None;
        }

        choose_best_source_result(
            self.lookup_provider_scoped_path_litellm(model_id, provider_id),
            self.lookup_provider_scoped_path_openrouter(model_id, provider_id),
            Some(scoped.provider),
        )
    }

    pub(in crate::pricing::lookup) fn lookup_provider_scoped_path_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let scoped = parse_provider_scoped_model_path(model_id)?;
        if !provider_hint_matches_scoped_provider(provider_id, scoped.provider) {
            return None;
        }

        if let Some(result) = self.exact_match_litellm(model_id) {
            return Some(result);
        }

        let scoped_tags = provider_identity::provider_tags(scoped.provider);
        for prefix in RESELLER_PROVIDER_PREFIXES {
            if !provider_prefix_matches_scoped_provider(prefix, &scoped_tags) {
                continue;
            }

            let key = format!("{}{}", prefix, model_id);
            if let Some(litellm_key) = self.litellm_lower.get(&key) {
                if let Some(pricing) = self.litellm.get(litellm_key) {
                    if let Some(result) = lookup_result_if_usable(pricing, "LiteLLM", litellm_key) {
                        return Some(result);
                    }
                }
            }
        }

        self.exact_match_litellm_for_provider(scoped.terminal_model_id, Some(scoped.provider))
    }

    pub(in crate::pricing::lookup) fn lookup_provider_scoped_path_openrouter(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let scoped = parse_provider_scoped_model_path(model_id)?;
        if !provider_hint_matches_scoped_provider(provider_id, scoped.provider) {
            return None;
        }

        self.exact_match_openrouter(model_id).or_else(|| {
            self.exact_match_openrouter_for_provider(
                scoped.terminal_model_id,
                Some(scoped.provider),
            )
        })
    }
}

pub(in crate::pricing::lookup) fn parse_provider_scoped_model_path(
    model_id: &str,
) -> Option<ProviderScopedModelPath<'_>> {
    let rest = model_id.strip_prefix("accounts/")?;
    let (provider, rest) = rest.split_once('/')?;
    let (scope, terminal_model_id) = rest.split_once('/')?;

    if provider.is_empty() || terminal_model_id.is_empty() {
        return None;
    }

    match scope {
        "models" | "routers" => Some(ProviderScopedModelPath {
            provider,
            terminal_model_id,
        }),
        _ => None,
    }
}
