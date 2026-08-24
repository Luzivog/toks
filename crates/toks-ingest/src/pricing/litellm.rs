use super::{cache, describe_error, fetch};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CACHE_FILENAME: &str = "pricing-litellm.json";
const PRICING_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelPricing {
    pub input_cost_per_token: Option<f64>,
    pub input_cost_per_token_above_128k_tokens: Option<f64>,
    pub input_cost_per_token_above_200k_tokens: Option<f64>,
    pub input_cost_per_token_above_256k_tokens: Option<f64>,
    pub input_cost_per_token_above_272k_tokens: Option<f64>,
    pub output_cost_per_token: Option<f64>,
    pub output_cost_per_token_above_128k_tokens: Option<f64>,
    pub output_cost_per_token_above_200k_tokens: Option<f64>,
    pub output_cost_per_token_above_256k_tokens: Option<f64>,
    pub output_cost_per_token_above_272k_tokens: Option<f64>,
    pub cache_creation_input_token_cost: Option<f64>,
    pub cache_creation_input_token_cost_above_200k_tokens: Option<f64>,
    pub cache_read_input_token_cost: Option<f64>,
    pub cache_read_input_token_cost_above_200k_tokens: Option<f64>,
    pub cache_read_input_token_cost_above_272k_tokens: Option<f64>,
}

impl ModelPricing {
    /// Every rate this row can carry, base rates and long-context tiers alike,
    /// as one list.
    ///
    /// The destructure below deliberately carries no `..` rest pattern: adding
    /// a field to `ModelPricing` breaks this function at compile time and
    /// forces the new rate into every predicate that reasons over *all* rates.
    /// Leaving that to a comment is not safe enough for
    /// `quotes_zero_for_every_published_rate`, where an overlooked rate is
    /// treated as zero and a new paid tier would read as free.
    pub(crate) fn all_rates(&self) -> [Option<f64>; 15] {
        let Self {
            input_cost_per_token,
            input_cost_per_token_above_128k_tokens,
            input_cost_per_token_above_200k_tokens,
            input_cost_per_token_above_256k_tokens,
            input_cost_per_token_above_272k_tokens,
            output_cost_per_token,
            output_cost_per_token_above_128k_tokens,
            output_cost_per_token_above_200k_tokens,
            output_cost_per_token_above_256k_tokens,
            output_cost_per_token_above_272k_tokens,
            cache_creation_input_token_cost,
            cache_creation_input_token_cost_above_200k_tokens,
            cache_read_input_token_cost,
            cache_read_input_token_cost_above_200k_tokens,
            cache_read_input_token_cost_above_272k_tokens,
        } = *self;

        [
            input_cost_per_token,
            input_cost_per_token_above_128k_tokens,
            input_cost_per_token_above_200k_tokens,
            input_cost_per_token_above_256k_tokens,
            input_cost_per_token_above_272k_tokens,
            output_cost_per_token,
            output_cost_per_token_above_128k_tokens,
            output_cost_per_token_above_200k_tokens,
            output_cost_per_token_above_256k_tokens,
            output_cost_per_token_above_272k_tokens,
            cache_creation_input_token_cost,
            cache_creation_input_token_cost_above_200k_tokens,
            cache_read_input_token_cost,
            cache_read_input_token_cost_above_200k_tokens,
            cache_read_input_token_cost_above_272k_tokens,
        ]
    }

    /// Whether every rate this row publishes is exactly `0.0`, so the buckets
    /// it never quotes can be read as zero as well.
    ///
    /// This is a statement about the row, not about the deal. Rows that quote
    /// `0.0` for the buckets upstream bothered to list and omit the rest reach
    /// us from two different worlds and are indistinguishable in the data:
    /// `opencode/nemotron-3-ultra-free` is a genuinely free row, and
    /// `kenari/claude-opus-4-7` is a premium model whose zeros mean
    /// "included in your subscription" — both arrive as
    /// `{input_cost_per_token: 0.0, output_cost_per_token: 0.0}` with null
    /// cache fields, byte for byte. Nothing in the dataset separates them, so
    /// Toks reports both at $0.00 and cannot do better from this input
    /// alone. If upstream ever publishes a plan/subscription marker, that is
    /// the signal to split these two cases apart.
    ///
    /// A zero row must not be read as "the model is free": kenari's entire
    /// catalog is subscription-priced at zero (all 38 of its rows in models.dev
    /// are `{0, 0}`, `grok-4-5` and `claude-fable-5` included), and the same
    /// nemotron has 14 paid siblings — `nvidia/nvidia/...` at $0.50/$2.50,
    /// `openrouter/...` at $0.60/$3.60 — so a resolution that lands on a
    /// subscription row prices a paid model at $0.00. That is a lookup-quality
    /// problem, tracked separately, not something this predicate decides.
    ///
    /// That limitation predates this predicate rather than arriving with it:
    /// `0.0` has always satisfied `valid_rate`, so a plan-priced row already
    /// covered — and already reported at $0.00 — any usage without cache
    /// tokens. What this widens is the usage shapes it covers, extending the
    /// same $0.00 answer to cache-bearing usage instead of aborting the
    /// submission that carries it (#1021, #1035).
    ///
    /// Two conditions, and both matter:
    ///
    /// The row must quote base rates for *both* input and output. Those are
    /// the two buckets every completion populates and every provider prices,
    /// so quoting zero for both is the strongest signal the data offers. One
    /// zero rate is not: a row quoting free input while omitting output has
    /// said nothing about generation, which is where the money is, and
    /// extrapolating from it would report a paid model at $0.00.
    ///
    /// Every rate the row quotes must then be zero, tiers included. A zero
    /// base rate beside a paid above-128k tier is a promotional tier rather
    /// than an all-zero row, and keeps the strict coverage check below.
    fn quotes_zero_for_every_published_rate(&self) -> bool {
        let quoted_rate =
            |rate: Option<f64>| rate.is_some_and(|rate| rate.is_finite() && rate >= 0.0);
        quoted_rate(self.input_cost_per_token)
            && quoted_rate(self.output_cost_per_token)
            && self
                .all_rates()
                .into_iter()
                .flatten()
                // Rejects NaN as well as any real price, so a row carrying an
                // unusable rate is never mistaken for a published zero.
                .all(|rate| rate == 0.0)
    }

    /// Whether this row can price every populated token bucket under
    /// `compute_cost`'s current base-rate fallback semantics. Explicit zeroes
    /// are valid prices; a missing base rate is not covered by a later tier.
    pub(crate) fn covers_usage(&self, usage: &crate::TokenBreakdown) -> bool {
        // A row that quotes zero for every rate it publishes prices the ones
        // it omits at zero too, so it covers any usage shape. `compute_cost`
        // already reads an absent rate as 0.0, so this only stops a $0.00
        // submission being rejected — it changes no total.
        //
        // Answering true here deliberately pre-empts `resolve_for_usage`'s
        // canonical-borrow path (#1013), which exists to fill omitted cache
        // rates from the unhinted row. Nothing is lost: an all-zero row can
        // only borrow from a canonical row quoting the same base rates, so
        // every rate it could take is zero and the total is unchanged.
        if self.quotes_zero_for_every_published_rate() {
            return true;
        }

        let valid_rate =
            |rate: Option<f64>| rate.is_some_and(|rate| rate.is_finite() && rate >= 0.0);
        (usage.input <= 0 || valid_rate(self.input_cost_per_token))
            && (usage.output <= 0 && usage.reasoning <= 0 || valid_rate(self.output_cost_per_token))
            && (usage.cache_read <= 0 || valid_rate(self.cache_read_input_token_cost))
            && (usage.cache_write <= 0 || valid_rate(self.cache_creation_input_token_cost))
    }

    /// A copy of this row with rates taken from `fallback` for the buckets
    /// `usage` populates but this row cannot price.
    ///
    /// No rate already present here is ever overwritten, including the
    /// long-context tiers: a row that publishes an above-threshold rate for a
    /// bucket whose base rate it omits keeps that tier, and only the rates it
    /// genuinely lacks are taken from `fallback`. Callers are responsible for
    /// establishing that the two rows price the same deal before borrowing.
    pub(crate) fn with_missing_rates_from(
        &self,
        fallback: &Self,
        usage: &crate::TokenBreakdown,
    ) -> Self {
        let valid_rate =
            |rate: Option<f64>| rate.is_some_and(|rate| rate.is_finite() && rate >= 0.0);
        let valid_or_fallback = |rate: Option<f64>, fallback_rate: Option<f64>| {
            rate.filter(|rate| rate.is_finite() && *rate >= 0.0)
                .or_else(|| fallback_rate.filter(|rate| rate.is_finite() && *rate >= 0.0))
        };
        let mut filled = self.clone();

        if usage.input > 0
            && !valid_rate(filled.input_cost_per_token)
            && valid_rate(fallback.input_cost_per_token)
        {
            filled.input_cost_per_token = fallback.input_cost_per_token;
            filled.input_cost_per_token_above_128k_tokens = valid_or_fallback(
                filled.input_cost_per_token_above_128k_tokens,
                fallback.input_cost_per_token_above_128k_tokens,
            );
            filled.input_cost_per_token_above_200k_tokens = valid_or_fallback(
                filled.input_cost_per_token_above_200k_tokens,
                fallback.input_cost_per_token_above_200k_tokens,
            );
            filled.input_cost_per_token_above_256k_tokens = valid_or_fallback(
                filled.input_cost_per_token_above_256k_tokens,
                fallback.input_cost_per_token_above_256k_tokens,
            );
            filled.input_cost_per_token_above_272k_tokens = valid_or_fallback(
                filled.input_cost_per_token_above_272k_tokens,
                fallback.input_cost_per_token_above_272k_tokens,
            );
        }

        if (usage.output > 0 || usage.reasoning > 0)
            && !valid_rate(filled.output_cost_per_token)
            && valid_rate(fallback.output_cost_per_token)
        {
            filled.output_cost_per_token = fallback.output_cost_per_token;
            filled.output_cost_per_token_above_128k_tokens = valid_or_fallback(
                filled.output_cost_per_token_above_128k_tokens,
                fallback.output_cost_per_token_above_128k_tokens,
            );
            filled.output_cost_per_token_above_200k_tokens = valid_or_fallback(
                filled.output_cost_per_token_above_200k_tokens,
                fallback.output_cost_per_token_above_200k_tokens,
            );
            filled.output_cost_per_token_above_256k_tokens = valid_or_fallback(
                filled.output_cost_per_token_above_256k_tokens,
                fallback.output_cost_per_token_above_256k_tokens,
            );
            filled.output_cost_per_token_above_272k_tokens = valid_or_fallback(
                filled.output_cost_per_token_above_272k_tokens,
                fallback.output_cost_per_token_above_272k_tokens,
            );
        }

        if usage.cache_read > 0
            && !valid_rate(filled.cache_read_input_token_cost)
            && valid_rate(fallback.cache_read_input_token_cost)
        {
            filled.cache_read_input_token_cost = fallback.cache_read_input_token_cost;
            filled.cache_read_input_token_cost_above_200k_tokens = valid_or_fallback(
                filled.cache_read_input_token_cost_above_200k_tokens,
                fallback.cache_read_input_token_cost_above_200k_tokens,
            );
            filled.cache_read_input_token_cost_above_272k_tokens = valid_or_fallback(
                filled.cache_read_input_token_cost_above_272k_tokens,
                fallback.cache_read_input_token_cost_above_272k_tokens,
            );
        }

        if usage.cache_write > 0
            && !valid_rate(filled.cache_creation_input_token_cost)
            && valid_rate(fallback.cache_creation_input_token_cost)
        {
            filled.cache_creation_input_token_cost = fallback.cache_creation_input_token_cost;
            filled.cache_creation_input_token_cost_above_200k_tokens = valid_or_fallback(
                filled.cache_creation_input_token_cost_above_200k_tokens,
                fallback.cache_creation_input_token_cost_above_200k_tokens,
            );
        }

        filled
    }

    pub(crate) fn has_any_usable_base_rate(&self) -> bool {
        [
            self.input_cost_per_token,
            self.output_cost_per_token,
            self.cache_creation_input_token_cost,
            self.cache_read_input_token_cost,
        ]
        .into_iter()
        .any(|rate| rate.is_some_and(|rate| rate.is_finite() && rate >= 0.0))
    }
}

pub type PricingDataset = HashMap<String, ModelPricing>;

#[cfg(test)]
#[path = "litellm_pricing_row_tests.rs"]
mod pricing_row_tests;

pub fn load_cached() -> Option<PricingDataset> {
    cache::load_cache(CACHE_FILENAME)
}

pub fn load_cached_any_age() -> Option<PricingDataset> {
    cache::load_cache_any_age(CACHE_FILENAME)
}

pub async fn fetch() -> Result<PricingDataset, String> {
    fetch_inner(PRICING_URL, true).await
}

/// `use_disk_cache` governs BOTH halves of the on-disk cache — the read below
/// and the write at the end. It used to gate only the read, so a caller that
/// asked for a fresh fetch still published the result to
/// `~/.config/tokscope/cache/pricing-litellm.json`. Any fixture-server test
/// that fetched successfully therefore published its stub over the real
/// multi-thousand-model dataset for a full TTL, and while it was clobbered
/// LiteLLM contributed nothing to pricing lookups (#1021, #1035).
///
/// Gating the write here rather than isolating the tests behind
/// `TOKSCOPE_CONFIG_DIR`: the override is process-global, so it only protects
/// the tests that remember to set it, and the next fixture-server test added
/// without it silently reintroduces the bug. A flag on the function makes the
/// opt-out total and local — the caller that declined the cache cannot write to
/// it by construction. The override still earns its keep in the regression
/// test, where it is the only way to observe the write without touching the
/// developer's real cache. No production caller wants read-bypass-with-write:
/// `fetch()` is the sole one and it passes `true`.
async fn fetch_inner(url: &str, use_disk_cache: bool) -> Result<PricingDataset, String> {
    if use_disk_cache {
        if let Some(cached) = load_cached() {
            return Ok(cached);
        }
    }

    let client = fetch::pricing_client()?;
    let response = fetch::get_with_retry(&client, url, "LiteLLM").await?;
    let mut data = response
        .json::<PricingDataset>()
        .await
        .map_err(|error| describe_error(&error))?;
    data.retain(|_, pricing| pricing.has_any_usable_base_rate());
    if data.is_empty() {
        return Err("LiteLLM returned no usable pricing rows".to_string());
    }
    if use_disk_cache {
        if let Err(e) = cache::save_cache(CACHE_FILENAME, &data) {
            eprintln!(
                "[toks] Warning: Failed to cache LiteLLM pricing at {}: {}",
                cache::get_cache_path(CACHE_FILENAME).display(),
                e
            );
        }
    }
    Ok(data)
}

#[cfg(test)]
#[path = "litellm_tests.rs"]
mod tests;
