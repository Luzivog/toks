use super::normalize::{normalize_model_name, normalize_version_separator};
use super::prefix_fuzzy::is_fuzzy_eligible;
use super::provider::{parse_provider_scoped_model_path, strip_known_provider_prefix};
use super::{LookupResult, PricingLookup};

impl PricingLookup {
    pub(in crate::pricing::lookup) fn exact_or_normalized_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_litellm_for_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(result) = self.exact_match_litellm(model_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_litellm_for_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
            if let Some(result) = self.exact_match_litellm(&version_normalized) {
                return Some(result);
            }
        }
        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) = self.exact_match_litellm_for_provider(&normalized, provider_id) {
                return Some(result);
            }
            if let Some(result) = self.exact_match_litellm(&normalized) {
                return Some(result);
            }
        }
        None
    }

    pub(in crate::pricing::lookup) fn lookup_models_dev_only(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(result) = self.exact_match_models_dev_with_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_models_dev_with_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) =
                self.exact_match_models_dev_with_provider(&normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(result) = self.prefix_match_models_dev(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_models_dev(&version_normalized, provider_id) {
                return Some(result);
            }
        }
        None
    }

    pub(in crate::pricing::lookup) fn lookup_litellm_only(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.lookup_provider_scoped_path_litellm(model_id, provider_id) {
            return Some(result);
        }
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(result) = self.exact_or_normalized_litellm(model_id, provider_id) {
            return Some(result);
        }
        if let Some(stripped) = strip_known_provider_prefix(model_id) {
            if let Some(result) = self.exact_or_normalized_litellm(stripped, provider_id) {
                return Some(result);
            }
        }
        if let Some(result) = self.prefix_match_litellm(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_litellm(&version_normalized, provider_id) {
                return Some(result);
            }
        }
        if is_fuzzy_eligible(model_id) {
            if let Some(result) = self.fuzzy_match_litellm(model_id, provider_id) {
                return Some(result);
            }
        }
        None
    }

    pub(in crate::pricing::lookup) fn lookup_openrouter_only(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.lookup_provider_scoped_path_openrouter(model_id, provider_id) {
            return Some(result);
        }
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(result) = self.exact_match_openrouter_with_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_openrouter_with_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) =
                self.exact_match_openrouter_with_provider(&normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(result) = self.prefix_match_openrouter(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_openrouter(&version_normalized, provider_id) {
                return Some(result);
            }
        }
        if is_fuzzy_eligible(model_id) {
            if let Some(result) = self.fuzzy_match_openrouter(model_id, provider_id) {
                return Some(result);
            }
        }
        None
    }
}
