pub mod aliases;
pub mod basis;
pub mod cache;
pub mod custom;
mod embedded;
#[cfg(test)]
mod embedded_tests;
mod environment;
mod fetch;
pub mod litellm;
pub mod lookup;
pub mod models_dev;
pub mod openrouter;
use crate::TokenBreakdown;
use custom::CustomPricing;
pub use litellm::ModelPricing;
use lookup::{compute_cost, LookupResult, PricingLookup};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use tokio::sync::OnceCell;

static PRICING_SERVICE: OnceCell<Arc<PricingService>> = OnceCell::const_new();
static REFRESH_STARTED: AtomicBool = AtomicBool::new(false);

// @keep: documents non-obvious filtering behavior — without this, the next person
// will wonder why github_copilot entries disappear from the pricing data.
/// Provider prefixes in LiteLLM data that use subscription-based pricing ($0.00)
/// and should be excluded from pay-per-token cost estimation.
const EXCLUDED_LITELLM_PREFIXES: &[&str] = &["github_copilot/"];

// @keep: explains why we do not just print the error.
/// Flatten an error and its `source()` chain into one line.
///
/// `reqwest::Error`'s `Display` is deliberately terse: a body-decode failure
/// renders as the bare string "error decoding response body", and the
/// `serde_json` cause that names the offending field and byte offset hangs off
/// `source()`, which `{}` never walks. Issue #1002 was reported with exactly
/// that message, which is why it was impossible to tell a transport failure
/// from an upstream schema change and the reporter guessed at TLS. Printing the
/// chain makes the next such report actionable.
pub(crate) fn describe_error(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(inner) = source {
        parts.push(inner.to_string());
        source = inner.source();
    }
    parts.join(": ")
}

pub struct PricingService {
    state: RwLock<PricingState>,
}

struct PricingState {
    custom: CustomPricing,
    lookup: PricingLookup,
}

impl PricingService {
    pub fn new(
        litellm_data: HashMap<String, ModelPricing>,
        openrouter_data: HashMap<String, ModelPricing>,
    ) -> Self {
        Self::new_with_custom(CustomPricing::default(), litellm_data, openrouter_data)
    }

    pub fn new_with_custom(
        custom: CustomPricing,
        litellm_data: HashMap<String, ModelPricing>,
        openrouter_data: HashMap<String, ModelPricing>,
    ) -> Self {
        Self::new_with_custom_and_models_dev(custom, litellm_data, openrouter_data, HashMap::new())
    }

    pub fn new_with_custom_and_models_dev(
        custom: CustomPricing,
        litellm_data: HashMap<String, ModelPricing>,
        openrouter_data: HashMap<String, ModelPricing>,
        models_dev_data: HashMap<String, ModelPricing>,
    ) -> Self {
        Self {
            state: RwLock::new(PricingState {
                custom,
                lookup: PricingLookup::new_with_models_dev(
                    litellm_data,
                    openrouter_data,
                    Self::build_cursor_overrides(),
                    Self::build_sakana_overrides(),
                    models_dev_data,
                ),
            }),
        }
    }

    fn new_with_embedded_baseline(
        custom: CustomPricing,
        litellm_data: HashMap<String, ModelPricing>,
        openrouter_data: HashMap<String, ModelPricing>,
        models_dev_data: HashMap<String, ModelPricing>,
    ) -> Self {
        let mut merged_models_dev = embedded::dataset();
        merged_models_dev.extend(models_dev_data);
        Self::new_with_custom_and_models_dev(
            custom,
            litellm_data,
            openrouter_data,
            merged_models_dev,
        )
    }

    // @keep: the retain logic is non-trivial (lowercase + prefix match); this doc
    // explains *why* these entries are dropped, not just *what* the code does.
    /// Filter out LiteLLM entries from subscription-based providers (e.g. github_copilot/)
    /// whose $0.00 pricing is meaningless for per-token cost estimation.
    fn filter_litellm_data(
        mut data: HashMap<String, ModelPricing>,
    ) -> HashMap<String, ModelPricing> {
        data.retain(|key, _| {
            let lower = key.to_lowercase();
            let included_provider = !EXCLUDED_LITELLM_PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(prefix));
            included_provider
        });
        data.retain(|_, pricing| pricing.has_any_usable_base_rate());
        data
    }

    // @keep: Cursor-sourced pricing for models not yet in LiteLLM/OpenRouter.
    // Checked after exact/prefix matches but before fuzzy matching in PricingLookup,
    // so real upstream entries (including provider-prefixed like openai/gpt-5.3-codex)
    // always win. Source citations are required for audit trail.
    fn build_cursor_overrides() -> HashMap<String, ModelPricing> {
        // @keep: the difference between `None` and `Some(0.0)` here is load-bearing.
        // The 5th field is cache CREATION. `None` means "rate unknown", and
        // `covers_usage` then reports the row as not covering any usage that
        // populates cache_write — which excludes it from submission entirely.
        // `Some(0.0)` means "documented free". `compute_cost` already reads an
        // absent rate as 0.0, so the two produce an identical cost; only the
        // coverage verdict differs. Set it ONLY where Cursor documents cache
        // creation as free — guessing a rate would invent spend.
        /// `(model id, input, output, cache read, cache creation)`, per token.
        ///
        /// Both cache rates distinguish "unknown" from "free": `None` means the
        /// rate is undocumented, `Some(0.0)` means Cursor publishes it as free.
        type CursorRateRow = (&'static str, f64, f64, Option<f64>, Option<f64>);

        let entries: &[CursorRateRow] = &[
            // GPT-5.3 family: $1.75/$14.00 per 1M tokens, $0.175 cache read
            // Source: Cursor docs (cursor.com/en-US/docs/models), llm-stats.com
            ("gpt-5.3", 0.00000175, 0.000014, Some(1.75e-7), None),
            ("gpt-5.3-codex", 0.00000175, 0.000014, Some(1.75e-7), None),
            (
                "gpt-5.3-codex-spark",
                0.00000175,
                0.000014,
                Some(1.75e-7),
                None,
            ),
            // Composer 1: $1.25/$10.00 per 1M tokens, $0.125 cache read
            // Source: Cursor docs (cursor.com/docs/models#model-pricing)
            ("composer 1", 0.00000125, 0.00001, Some(1.25e-7), None),
            ("composer-1", 0.00000125, 0.00001, Some(1.25e-7), None),
            // Composer 1.5: $3.50/$17.50 per 1M tokens, $0.35 cache read
            // Source: Cursor docs (cursor.com/docs/models#model-pricing), issue #276
            ("composer 1.5", 0.0000035, 0.0000175, Some(3.5e-7), None),
            ("composer-1.5", 0.0000035, 0.0000175, Some(3.5e-7), None),
            // Composer 2: $0.50/$2.50 per 1M input/output, $0.20/M cache read; cache creation free
            // Composer 2 Fast: $1.50/$7.50 per 1M, $0.35/M cache read; cache creation free
            // Source: Cursor docs (cursor.com/docs/models#model-pricing)
            ("composer 2", 5e-7, 2.5e-6, Some(2e-7), Some(0.0)),
            ("composer-2", 5e-7, 2.5e-6, Some(2e-7), Some(0.0)),
            ("composer 2 fast", 1.5e-6, 7.5e-6, Some(3.5e-7), Some(0.0)),
            ("composer-2-fast", 1.5e-6, 7.5e-6, Some(3.5e-7), Some(0.0)),
            // Composer 2: $0.50/$2.50 per 1M input/output, $0.20/M cache read; cache creation free
            // Composer 2 Fast: $1.50/$7.50 per 1M, $0.35/M cache read; cache creation free
            // Source: Cursor docs (cursor.com/docs/models#model-pricing)
            ("composer-2.5", 5e-7, 2.5e-6, Some(2e-7), Some(0.0)),
            ("composer-2.5-fast", 1.5e-6, 7.5e-6, Some(3.5e-7), Some(0.0)),
        ];

        let mut overrides = HashMap::with_capacity(entries.len());
        for (model_id, input, output, cache_read, cache_creation) in entries {
            overrides.insert(
                model_id.to_string(),
                ModelPricing {
                    input_cost_per_token: Some(*input),
                    output_cost_per_token: Some(*output),
                    cache_read_input_token_cost: *cache_read,
                    cache_creation_input_token_cost: *cache_creation,
                    ..Default::default()
                },
            );
        }
        overrides
    }

    // @keep: Sakana-sourced pricing for `fugu-ultra`, a model not carried by
    // LiteLLM/OpenRouter/models.dev. Reports source label "Sakana" (NOT "Cursor")
    // and is consulted at the same precedence as the Cursor overrides in
    // PricingLookup — after exact/normalized/prefix upstream matches, before the
    // fuzzy stage — so any real upstream entry always wins. The ModelPricing
    // struct is built directly (not via the 4-tuple shorthand) so the >272K
    // long-context tier fields can be populated; compute_cost DOES read those
    // *_above_272k_tokens fields when input/output/cache-read token volume
    // crosses 272K, so they are live, not inert.
    //
    // Rates source: https://console.sakana.ai/pricing and https://sakana.ai/fugu/
    // (accessed 2026-06-22).
    //   fugu-ultra base: input $5/1M, output $30/1M, cache-read $0.50/1M.
    //   fugu-ultra >272K-context tier: input $10/1M, output $45/1M, cache-read $1/1M.
    //
    // NOTE: there is deliberately NO `fugu` (non-ultra) entry. `fugu` is a
    // router/orchestrator billed at "the standard rate of the underlying
    // top-tier model involved" (https://sakana.ai/fugu/, accessed 2026-06-22):
    // the effective rate is variable per request and is NOT recoverable from the
    // session log, which only records model="fugu" with no record of which
    // underlying model actually served the request. Assigning any fixed
    // per-token rate to bare `fugu` would therefore be incorrect, so it is left
    // unpriced (callers fall through to the normal lookup chain / report no price).
    fn build_sakana_overrides() -> HashMap<String, ModelPricing> {
        let mut overrides = HashMap::with_capacity(1);
        overrides.insert(
            "fugu-ultra".to_string(),
            ModelPricing {
                // Base rates.
                input_cost_per_token: Some(5e-6),
                output_cost_per_token: Some(3e-5),
                cache_read_input_token_cost: Some(5e-7),
                cache_creation_input_token_cost: None,
                // >272K-context tier (consumed by compute_cost's tiered walk).
                input_cost_per_token_above_272k_tokens: Some(1e-5),
                output_cost_per_token_above_272k_tokens: Some(4.5e-5),
                cache_read_input_token_cost_above_272k_tokens: Some(1e-6),
                ..Default::default()
            },
        );
        overrides
    }

    async fn fetch_inner() -> Result<Self, String> {
        let (litellm_result, models_dev_result) =
            tokio::join!(litellm::fetch(), models_dev::fetch());

        // OpenRouter's catalog requires one request per model after listing
        // the catalog. That fan-out is far too expensive for a background
        // refresh, so retain any existing cache while the two compact catalogs
        // refresh in parallel.
        let openrouter_data = Ok(openrouter::load_cached_any_age().unwrap_or_default());

        Self::combine_fetched_sources(
            litellm_result,
            openrouter_data,
            models_dev_result,
            litellm::load_cached_any_age,
            openrouter::load_cached_any_age,
            models_dev::load_cached_any_age,
            CustomPricing::load_from_default_path(),
        )
    }

    /// Degrade one failed source to its own stale cache, else to nothing.
    fn degrade_source(
        label: &str,
        result: Result<HashMap<String, ModelPricing>, String>,
        cached: impl FnOnce() -> Option<HashMap<String, ModelPricing>>,
    ) -> HashMap<String, ModelPricing> {
        match result {
            Ok(data) => data,
            Err(error) => {
                let cached = cached();
                eprintln!(
                    "[toks] Warning: {} pricing fetch failed ({}); {}",
                    label,
                    error,
                    if cached.is_some() {
                        "falling back to the cached copy"
                    } else {
                        "continuing with the remaining pricing sources"
                    }
                );
                cached.unwrap_or_default()
            }
        }
    }

    // @keep: the asymmetry this removes was load-bearing and non-obvious.
    /// Assemble a service from whatever the three upstream sources returned.
    ///
    /// No single source may be fatal. LiteLLM is the largest dataset, but it is
    /// not the only one, and propagating its fetch error made every command
    /// that prices tokens — `submit` included — dead in the water whenever
    /// raw.githubusercontent.com was unreachable or served something we could
    /// not decode (#1002). Every dynamic source now preserves fetch failure as
    /// an error here, degrades to its own stale cache, and finally to nothing;
    /// the surviving sources still price what they cover. Submission safety is
    /// checked against the actual filtered messages later, rather than treating
    /// an empty dynamic dataset as a construction failure: custom and bundled
    /// pricing remain useful during an outage.
    fn combine_fetched_sources(
        litellm_result: Result<HashMap<String, ModelPricing>, String>,
        openrouter_result: Result<HashMap<String, ModelPricing>, String>,
        models_dev_result: Result<HashMap<String, ModelPricing>, String>,
        litellm_cached: impl FnOnce() -> Option<HashMap<String, ModelPricing>>,
        openrouter_cached: impl FnOnce() -> Option<HashMap<String, ModelPricing>>,
        models_dev_cached: impl FnOnce() -> Option<HashMap<String, ModelPricing>>,
        custom: CustomPricing,
    ) -> Result<Self, String> {
        let litellm_data = Self::filter_litellm_data(Self::degrade_source(
            "LiteLLM",
            litellm_result,
            litellm_cached,
        ));
        let models_dev_data =
            Self::degrade_source("models.dev", models_dev_result, models_dev_cached);
        let openrouter_data =
            Self::degrade_source("OpenRouter", openrouter_result, openrouter_cached);

        Ok(Self::new_with_embedded_baseline(
            custom,
            litellm_data,
            openrouter_data,
            models_dev_data,
        ))
    }

    fn from_cached_datasets(
        litellm_data: Option<HashMap<String, ModelPricing>>,
        openrouter_data: Option<HashMap<String, ModelPricing>>,
        models_dev_data: Option<HashMap<String, ModelPricing>>,
    ) -> Option<Self> {
        Some(Self::new_with_embedded_baseline(
            CustomPricing::load_from_default_path(),
            Self::filter_litellm_data(litellm_data.unwrap_or_default()),
            openrouter_data.unwrap_or_default(),
            models_dev_data.unwrap_or_default(),
        ))
    }

    /// True when at least the embedded baseline, a cached catalog, or custom
    /// pricing is available. The embedded OpenAI/Anthropic baseline means the
    /// default local service is usable even before a network refresh.
    pub fn has_pricing_data(&self) -> bool {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        !state.custom.is_empty() || state.lookup.has_upstream_dataset()
    }

    pub fn load_cached_any_age() -> Option<Self> {
        Self::from_cached_datasets(
            litellm::load_cached_any_age(),
            openrouter::load_cached_any_age(),
            models_dev::load_cached_any_age(),
        )
    }

    pub async fn get_or_init() -> Result<Arc<PricingService>, String> {
        let service = PRICING_SERVICE
            .get_or_init(|| async {
                Arc::new(
                    Self::load_cached_any_age()
                        .expect("the embedded pricing baseline is always available"),
                )
            })
            .await;
        Self::start_background_refresh(Arc::clone(service));
        Ok(Arc::clone(service))
    }

    fn start_background_refresh(service: Arc<Self>) {
        let cache_only = environment::cache_only();
        if cache_only || REFRESH_STARTED.swap(true, Ordering::AcqRel) {
            return;
        }

        let spawn = std::thread::Builder::new()
            .name("toks-pricing-refresh".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("[toks] Warning: pricing refresh could not start: {error}");
                        return;
                    }
                };
                match runtime.block_on(Self::fetch_inner()) {
                    Ok(fresh) => service.replace_with(fresh),
                    Err(error) => {
                        eprintln!("[toks] Warning: pricing refresh failed: {error}")
                    }
                }
            });
        if let Err(error) = spawn {
            REFRESH_STARTED.store(false, Ordering::Release);
            eprintln!("[toks] Warning: pricing refresh thread failed: {error}");
        }
    }

    fn replace_with(&self, fresh: Self) {
        let fresh = fresh
            .state
            .into_inner()
            .unwrap_or_else(|error| error.into_inner());
        *self
            .state
            .write()
            .unwrap_or_else(|error| error.into_inner()) = fresh;
    }

    pub fn lookup_with_source(
        &self,
        model_id: &str,
        force_source: Option<&str>,
    ) -> Option<LookupResult> {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        match force_source {
            Some(source) if source.eq_ignore_ascii_case("custom") => {
                return Self::lookup_custom(&state, model_id);
            }
            None => {
                if let Some(result) = Self::lookup_custom(&state, model_id) {
                    return Some(result);
                }
            }
            Some(_) => {}
        }

        state.lookup.lookup_with_source(model_id, force_source)
    }

    pub fn lookup_with_source_and_provider(
        &self,
        model_id: &str,
        force_source: Option<&str>,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        match force_source {
            Some(source) if source.eq_ignore_ascii_case("custom") => {
                return Self::lookup_custom(&state, model_id);
            }
            None => {
                if let Some(result) = Self::lookup_custom(&state, model_id) {
                    return Some(result);
                }
            }
            Some(_) => {}
        }

        state
            .lookup
            .lookup_with_source_and_provider(model_id, force_source, provider_id)
    }

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
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        if let Some(result) = state.custom.lookup_with_key(model_id) {
            return compute_cost(
                result.pricing,
                usage.input,
                usage.output,
                usage.cache_read,
                usage.cache_write,
                usage.reasoning,
            );
        }

        state
            .lookup
            .calculate_cost_with_provider(model_id, provider_id, usage)
    }

    pub fn covers_usage_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
    ) -> bool {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        if let Some(result) = state.custom.lookup_with_key(model_id) {
            return result.pricing.covers_usage(usage);
        }

        state
            .lookup
            .covers_usage_with_provider(model_id, provider_id, usage)
    }

    fn lookup_custom(state: &PricingState, model_id: &str) -> Option<LookupResult> {
        state
            .custom
            .lookup_with_key(model_id)
            .map(|result| LookupResult {
                pricing: result.pricing.clone(),
                source: "Custom".into(),
                matched_key: result.matched_key.to_string(),
            })
    }
}

#[cfg(test)]
mod tests;
