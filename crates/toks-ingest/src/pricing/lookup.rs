mod cache;
mod catalog;
mod state;

mod cost;
mod exact;
mod normalize;
mod prefix_fuzzy;
mod provider;
mod select;

mod source;

pub use cost::compute_cost;
pub(crate) use provider::is_routing_label;
pub use state::{LookupResult, PricingLookup};

use normalize::NormalizedRequest;
use prefix_fuzzy::{try_strip_unknown_prefix, try_strip_unknown_suffix};
use provider::{
    lookup_known_provider_prefix, normalize_provider_hint, parse_provider_scoped_model_path,
    strip_generic_provider_prefix,
};

#[cfg(test)]
use super::litellm::ModelPricing;
#[cfg(test)]
use crate::TokenBreakdown;
#[cfg(test)]
use std::collections::HashMap;

impl PricingLookup {
    pub fn lookup_with_source(
        &self,
        model_id: &str,
        force_source: Option<&str>,
    ) -> Option<LookupResult> {
        self.lookup_with_source_and_provider(model_id, force_source, None)
    }

    pub fn lookup_with_source_and_provider(
        &self,
        model_id: &str,
        force_source: Option<&str>,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let provider_id = normalize_provider_hint(provider_id);
        let request = NormalizedRequest::new(model_id)?;
        let model_id = request.model_id();

        let do_lookup = |id: &str| match force_source {
            Some("litellm") => self.lookup_litellm_only(id, provider_id),
            Some("openrouter") => self.lookup_openrouter_only(id, provider_id),
            Some("models.dev") | Some("modelsdev") | Some("models_dev") => {
                self.lookup_models_dev_only(id, provider_id)
            }
            _ => self.lookup_auto(id, provider_id),
        };

        // 1. Direct lookup. An unsafe Claude resolution is a hard miss rather
        // than permission to continue into a weaker stage.
        if let Some(result) = do_lookup(model_id) {
            return request.allows(&result).then_some(result);
        }

        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        let guarded_lookup =
            |candidate: &str| do_lookup(candidate).filter(|result| request.allows(result));

        // 2. Generic provider-routing prefix fallback: ids coming from a
        // router/proxy (e.g. `cx/gpt-5.5` via an `omniroute` provider) carry a
        // prefix outside the curated provider-prefix list. Direct lookup above
        // already tried the full id, so a real prefixed dataset key resolves
        // before this fallback. Only the terminal segment is retried here.
        if let Some(terminal) = strip_generic_provider_prefix(model_id) {
            // Reaching here means no dataset key matched the qualified id. A
            // real `morph/auto` resolved earlier; a made-up `cx/auto` must not
            // be billed as Morph after dropping the unknown prefix.
            if is_routing_label(terminal) {
                return None;
            }
            if let Some(result) = guarded_lookup(terminal) {
                return Some(result);
            }
            // Prefix and suffix stripping must compose here. The suffix stage
            // below sees `cx/gpt-5.5-xhigh`, strips to `cx/gpt-5.5`, and still
            // misses. Retrying the terminal first is what lets #846 resolve.
            if let Some(result) = try_strip_unknown_suffix(terminal, guarded_lookup) {
                return Some(result);
            }
        }

        // 3. Strip unknown suffixes such as -thinking, -high, and -codex.
        if let Some(result) = try_strip_unknown_suffix(model_id, guarded_lookup) {
            return Some(result);
        }

        // 4. Strip unknown routing prefixes, composing with suffix stripping.
        try_strip_unknown_prefix(model_id, guarded_lookup)
    }

    fn lookup_auto(&self, model_id: &str, provider_id: Option<&str>) -> Option<LookupResult> {
        // 1. Provider-scoped path.
        if let Some(result) = self.lookup_provider_scoped_path(model_id, provider_id) {
            return Some(result);
        }
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        // 2. Known provider prefix.
        if let Some(result) = lookup_known_provider_prefix(self, model_id, provider_id) {
            return Some(result);
        }

        // 3. Exact and normalized exact matching.
        if let Some(result) = exact::lookup(self, model_id, provider_id) {
            return Some(result);
        }

        // 4. Prefix matching.
        if let Some(result) = prefix_fuzzy::lookup_prefix(self, model_id, provider_id) {
            return Some(result);
        }

        // 5. Repository-owned exact overrides.
        if let Some(result) = exact::lookup_overrides(self, model_id) {
            return Some(result);
        }

        // 6. Fuzzy matching and source arbitration.
        prefix_fuzzy::lookup_fuzzy(self, model_id, provider_id)
    }
}

#[cfg(test)]
mod tests;
