use super::ModelPricing;
use crate::TokenBreakdown;

const INPUT_THRESHOLDS: [i64; 4] = [128_000, 200_000, 256_000, 272_000];
const CACHE_READ_THRESHOLDS: [i64; 2] = [200_000, 272_000];
const CACHE_WRITE_THRESHOLDS: [i64; 1] = [200_000];

/// Additive request-level facts needed to reprice compact usage rollups.
///
/// Each request is split before aggregation, so nonlinear context tariffs are
/// never inferred from a minute, day, or month total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PricingBasis {
    pub input: [i64; 5],
    pub output: [i64; 5],
    pub cache_read: [i64; 3],
    pub cache_write: [i64; 2],
}

impl PricingBasis {
    pub fn from_usage(usage: &TokenBreakdown) -> Self {
        Self {
            input: split::<5>(usage.input, &INPUT_THRESHOLDS),
            output: split::<5>(
                usage.output.max(0).saturating_add(usage.reasoning.max(0)),
                &INPUT_THRESHOLDS,
            ),
            cache_read: split::<3>(usage.cache_read, &CACHE_READ_THRESHOLDS),
            cache_write: split::<2>(usage.cache_write, &CACHE_WRITE_THRESHOLDS),
        }
    }

    pub fn add_assign(&mut self, other: Self) {
        add(&mut self.input, other.input);
        add(&mut self.output, other.output);
        add(&mut self.cache_read, other.cache_read);
        add(&mut self.cache_write, other.cache_write);
    }
}

impl super::PricingService {
    pub fn calculate_basis_cost_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
        basis: &PricingBasis,
        long_context: bool,
    ) -> f64 {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        if let Some(result) = state.custom.lookup_with_key(model_id) {
            return compute_basis_cost(result.pricing, basis);
        }
        state.lookup.calculate_basis_cost_with_provider(
            model_id,
            provider_id,
            usage,
            basis,
            long_context,
        )
    }
}

pub(crate) fn compute_basis_cost(pricing: &ModelPricing, basis: &PricingBasis) -> f64 {
    dot(
        basis.input,
        rates(
            pricing.input_cost_per_token,
            [
                pricing.input_cost_per_token_above_128k_tokens,
                pricing.input_cost_per_token_above_200k_tokens,
                pricing.input_cost_per_token_above_256k_tokens,
                pricing.input_cost_per_token_above_272k_tokens,
            ],
        ),
    ) + dot(
        basis.output,
        rates(
            pricing.output_cost_per_token,
            [
                pricing.output_cost_per_token_above_128k_tokens,
                pricing.output_cost_per_token_above_200k_tokens,
                pricing.output_cost_per_token_above_256k_tokens,
                pricing.output_cost_per_token_above_272k_tokens,
            ],
        ),
    ) + dot(
        basis.cache_read,
        rates(
            pricing.cache_read_input_token_cost,
            [
                pricing.cache_read_input_token_cost_above_200k_tokens,
                pricing.cache_read_input_token_cost_above_272k_tokens,
            ],
        ),
    ) + dot(
        basis.cache_write,
        rates(
            pricing.cache_creation_input_token_cost,
            [pricing.cache_creation_input_token_cost_above_200k_tokens],
        ),
    )
}

fn split<const N: usize>(tokens: i64, thresholds: &[i64]) -> [i64; N] {
    debug_assert_eq!(thresholds.len() + 1, N);
    let mut bands = [0; N];
    let mut remaining = tokens.max(0);
    let mut lower = 0;
    for (index, threshold) in thresholds.iter().copied().enumerate() {
        let width = threshold.saturating_sub(lower);
        bands[index] = remaining.min(width);
        remaining = remaining.saturating_sub(width).max(0);
        lower = threshold;
    }
    bands[N - 1] = remaining;
    bands
}

fn rates<const B: usize, const T: usize>(base: Option<f64>, tiers: [Option<f64>; T]) -> [f64; B] {
    debug_assert_eq!(B, T + 1);
    let mut active = valid(base).unwrap_or(0.0);
    let mut result = [0.0; B];
    result[0] = active;
    for (index, tier) in tiers.into_iter().enumerate() {
        if let Some(rate) = valid(tier) {
            active = rate;
        }
        result[index + 1] = active;
    }
    result
}

fn dot<const N: usize>(tokens: [i64; N], rates: [f64; N]) -> f64 {
    tokens
        .into_iter()
        .zip(rates)
        .map(|(tokens, rate)| tokens.max(0) as f64 * rate)
        .sum()
}

fn add<const N: usize>(left: &mut [i64; N], right: [i64; N]) {
    for (left, right) in left.iter_mut().zip(right) {
        *left = left.saturating_add(right);
    }
}

fn valid(rate: Option<f64>) -> Option<f64> {
    rate.filter(|rate| rate.is_finite() && *rate >= 0.0)
}

#[cfg(test)]
#[path = "basis_tests.rs"]
mod tests;
