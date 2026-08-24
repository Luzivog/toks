use super::super::{provider::normalize_provider_hint, LookupResult, PricingLookup};
use super::compute::{compute_basis_cost_for_lookup, compute_cost_for_lookup};
use crate::pricing::{basis::PricingBasis, litellm::ModelPricing};
use crate::TokenBreakdown;

impl PricingLookup {
    pub fn calculate_cost(
        &self,
        model_id: &str,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
        reasoning: i64,
    ) -> f64 {
        let usage = TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            reasoning,
        };
        self.calculate_cost_with_provider(model_id, None, &usage)
    }

    pub fn calculate_cost_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
    ) -> f64 {
        let provider_id = normalize_provider_hint(provider_id);
        let result = match self.resolve_for_usage(model_id, provider_id, usage) {
            Some(r) => r,
            None => return 0.0,
        };

        compute_cost_for_lookup(&result, provider_id, usage)
    }

    pub(crate) fn calculate_basis_cost_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
        basis: &PricingBasis,
        long_context: bool,
    ) -> f64 {
        let provider_id = normalize_provider_hint(provider_id);
        let Some(result) = self.resolve_for_usage(model_id, provider_id, usage) else {
            return 0.0;
        };
        compute_basis_cost_for_lookup(&result, provider_id, basis, long_context)
    }

    pub(crate) fn covers_usage_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
    ) -> bool {
        self.resolve_for_usage(model_id, provider_id, usage)
            .is_some_and(|result| result.pricing.covers_usage(usage))
    }

    /// Resolve `model_id` for pricing `usage`, borrowing the rates the
    /// provider-hinted row omits from the canonical unhinted row.
    ///
    /// A provider hint can steer resolution onto a gateway or reseller key
    /// that lists input and output rates only — OpenRouter's
    /// `openai/gpt-5.2-codex` and LiteLLM's `gmi/google/gemini-3-pro-preview`
    /// both do — while the canonical key for the same model publishes the
    /// cache rates as well. Pricing the hinted row alone bills cached tokens
    /// at zero and makes `covers_usage` false, which aborted whole
    /// submissions for every Codex session (#1013).
    ///
    /// Only buckets the hinted row cannot price are filled, so a reseller row
    /// keeps its own markup rather than silently repricing to the author's
    /// cheaper rate. If the filled row still cannot cover the usage, the
    /// hinted row is returned unchanged and the usage stays unpriced.
    fn resolve_for_usage(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
    ) -> Option<LookupResult> {
        let hinted = self.lookup_with_provider(model_id, provider_id)?;
        if normalize_provider_hint(provider_id).is_none() || hinted.pricing.covers_usage(usage) {
            return Some(hinted);
        }

        let Some(canonical) = self.lookup_with_provider(model_id, None) else {
            return Some(hinted);
        };
        if canonical.matched_key == hinted.matched_key
            || !quote_same_base_rates(&hinted.pricing, &canonical.pricing)
        {
            return Some(hinted);
        }

        let filled = hinted
            .pricing
            .with_missing_rates_from(&canonical.pricing, usage);
        if !filled.covers_usage(usage) {
            return Some(hinted);
        }

        // Keep the hinted row's source and matched key: `compute_cost_for_lookup`
        // branches on both for OpenAI's full-request 272k tiering, so borrowing
        // rates must not change which pricing model applies.
        Some(LookupResult {
            pricing: filled,
            ..hinted
        })
    }
}

/// Whether two rows price the same deal, judged on the base rates they both
/// publish.
///
/// Borrowing a rate across rows that disagree would invent a tariff neither
/// provider charges: `azure_ai/grok-code-fast-1` bills $3.50/$17.50 per
/// million with no cache-read rate, while the canonical `xai/` row bills
/// $0.20/$1.50 with one, so an Azure row must never inherit xAI's cache
/// price. Rows must also agree on at least one bucket — without a single
/// shared rate there is no evidence they describe the same deal at all.
fn quote_same_base_rates(hinted: &ModelPricing, canonical: &ModelPricing) -> bool {
    let mut shared = false;

    for (hinted_rate, canonical_rate) in [
        (hinted.input_cost_per_token, canonical.input_cost_per_token),
        (
            hinted.output_cost_per_token,
            canonical.output_cost_per_token,
        ),
        (
            hinted.cache_read_input_token_cost,
            canonical.cache_read_input_token_cost,
        ),
        (
            hinted.cache_creation_input_token_cost,
            canonical.cache_creation_input_token_cost,
        ),
    ] {
        let (Some(hinted_rate), Some(canonical_rate)) = (hinted_rate, canonical_rate) else {
            continue;
        };
        if !hinted_rate.is_finite() || !canonical_rate.is_finite() {
            return false;
        }
        if (hinted_rate - canonical_rate).abs() > canonical_rate.abs() * 1e-9 {
            return false;
        }
        shared = true;
    }

    shared
}
