use super::{
    claude_family, contains_delimited_modern_major_minor, requested_claude_version,
    resolves_unsafe_claude_version,
};
use crate::pricing::aliases;
use crate::pricing::lookup::provider::is_routing_label;
use crate::pricing::lookup::LookupResult;
use crate::strip_parenthesized_reasoning_tier;

pub(in crate::pricing::lookup) struct NormalizedRequest {
    model_id: String,
    requested_family: Option<&'static str>,
    requested_version: Option<String>,
    unparsed_modern_version: bool,
}

impl NormalizedRequest {
    pub(in crate::pricing::lookup) fn new(model_id: &str) -> Option<Self> {
        // A router is not a model. Resolving one by model-part match elects
        // whatever unrelated vendor publishes the same word, and the result is
        // billed as if it were the real thing (#1062).
        if is_routing_label(model_id) {
            return None;
        }

        let canonical = aliases::resolve_alias(model_id).unwrap_or(model_id);
        let lower = canonical.to_lowercase();

        // CLIProxyAPI strips `(level)` reasoning-effort suffixes before routing,
        // so for pricing lookup we resolve to the base model regardless of tier.
        // Mirrors the dash-suffix path (e.g. `-xhigh`), which is handled by
        // `try_strip_unknown_suffix` in the outer pipeline.
        let normalized_owned = strip_parenthesized_reasoning_tier(&lower).map(str::to_owned);

        // A tier suffix does not turn a router into a model: `auto(high)`
        // normalizes to `auto` below and would otherwise reach the model-part
        // fallback and elect an unrelated vendor, exactly as the bare form did.
        if normalized_owned.as_deref().is_some_and(is_routing_label) {
            return None;
        }

        // Guard against silent misresolution: if the input ends with `(...)`
        // but the contents are not a recognized CLIProxyAPI level, refuse the
        // lookup. Falling through to suffix stripping could match a shorter,
        // unrelated model id by peeling the parenthesized fragment off (e.g.
        // `gpt-5.2-codex(invalid)` would resolve to `gpt-5.2`).
        if normalized_owned.is_none()
            && lower
                .strip_suffix(')')
                .and_then(|inner| inner.rsplit_once('('))
                .is_some()
        {
            return None;
        }

        let model_id = normalized_owned.unwrap_or(lower);
        let requested_family = claude_family(&model_id);
        let requested_version = requested_claude_version(&model_id);
        let unparsed_modern_version = requested_family.is_some()
            && requested_version.is_none()
            && contains_delimited_modern_major_minor(&model_id);

        Some(Self {
            model_id,
            requested_family,
            requested_version,
            unparsed_modern_version,
        })
    }

    pub(in crate::pricing::lookup) fn model_id(&self) -> &str {
        &self.model_id
    }

    pub(in crate::pricing::lookup) fn allows(&self, result: &LookupResult) -> bool {
        !resolves_unsafe_claude_version(
            self.requested_family,
            self.requested_version.as_deref(),
            self.unparsed_modern_version,
            result,
        )
    }
}
