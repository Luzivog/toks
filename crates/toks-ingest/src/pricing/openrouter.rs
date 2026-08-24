use super::litellm::ModelPricing;
use super::{cache, describe_error, fetch};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

const CACHE_FILENAME: &str = "pricing-openrouter.json";
/// Root of the OpenRouter REST API. Both requests this module makes — the
/// model list and the per-model endpoint lookup — are built from this one
/// value so a test can point the whole fetch at a local fixture server. The
/// per-model URL used to be hardcoded, which left the author-pricing leg
/// unreachable offline: any test that got far enough to exercise it made a
/// real request to openrouter.ai.
///
/// Do not reintroduce a literal URL for either request.
/// `a_fetch_with_caching_disabled_writes_no_cache_file` reaches the
/// author-pricing leg on purpose — that is the only path that populates
/// `result`, and therefore the only one that can reach the cache write the
/// test guards. Hardcoding that URL again would not fail the test; it would
/// make `cargo test` call openrouter.ai for real on every run.
const API_BASE: &str = "https://openrouter.ai/api/v1";
const MAX_CONCURRENT_REQUESTS: usize = 10;

/// Structs for `/api/v1/models` endpoint (list all models).

#[derive(Deserialize)]
struct ModelListPricing {
    prompt: String,
    completion: String,
}

#[derive(Deserialize)]
struct ModelListItem {
    id: String,
    pricing: Option<ModelListPricing>,
}

#[derive(Deserialize)]
struct ModelsListResponse {
    data: Vec<ModelListItem>,
}

/// Structs for `/api/v1/models/{id}/endpoints` endpoint (author pricing).

#[derive(Deserialize)]
struct EndpointPricing {
    prompt: String,
    completion: String,
    #[serde(default)]
    input_cache_read: Option<String>,
    #[serde(default)]
    input_cache_write: Option<String>,
}

#[derive(Deserialize)]
struct Endpoint {
    provider_name: String,
    pricing: EndpointPricing,
}

#[derive(Deserialize)]
struct EndpointData {
    #[allow(dead_code)]
    id: String,
    endpoints: Vec<Endpoint>,
}

#[derive(Deserialize)]
struct EndpointsResponse {
    data: EndpointData,
}

/// Model ID prefix to provider name mapping.
///
/// Translates model ID prefixes like `z-ai` to their corresponding
/// provider names in the endpoints API, such as `Z.AI`.
fn get_author_provider_name(model_id: &str) -> Option<&'static str> {
    let prefix = model_id.split('/').next()?;

    match prefix.to_lowercase().as_str() {
        "z-ai" => Some("Z.AI"),
        "x-ai" => Some("xAI"),
        "anthropic" => Some("Anthropic"),
        "openai" => Some("OpenAI"),
        "google" => Some("Google"),
        "meta-llama" => Some("Meta"),
        "mistralai" => Some("Mistral"),
        "deepseek" => Some("DeepSeek"),
        "qwen" => Some("Alibaba"),
        "cohere" => Some("Cohere"),
        "perplexity" => Some("Perplexity"),
        "moonshotai" => Some("Moonshot AI"),
        _ => None,
    }
}

pub fn load_cached() -> Option<HashMap<String, ModelPricing>> {
    cache::load_cache(CACHE_FILENAME)
}

pub fn load_cached_any_age() -> Option<HashMap<String, ModelPricing>> {
    cache::load_cache_any_age(CACHE_FILENAME)
}

fn parse_price(s: &str) -> Option<f64> {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}

async fn fetch_author_pricing(
    client: Arc<reqwest::Client>,
    api_base: Arc<String>,
    model_id: String,
    semaphore: Arc<Semaphore>,
    fallback_pricing: Option<ModelPricing>,
) -> Option<(String, ModelPricing)> {
    let _permit = semaphore.acquire().await.ok()?;

    let author_name = match get_author_provider_name(&model_id) {
        Some(name) => name,
        None => return fallback_pricing.map(|p| (model_id, p)),
    };

    let url = format!("{}/models/{}/endpoints", api_base, model_id);

    let response = match client
        .get(&url)
        .header("Content-Type", "application/json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            return fallback_pricing.map(|p| (model_id, p));
        }
    };

    if !response.status().is_success() {
        return fallback_pricing.map(|p| (model_id, p));
    }

    let data: EndpointsResponse = match response.json().await {
        Ok(d) => d,
        Err(_) => {
            return fallback_pricing.map(|p| (model_id, p));
        }
    };

    match select_endpoint_pricing(&data.data.endpoints, author_name, fallback_pricing.as_ref()) {
        Some(pricing) => Some((model_id, pricing)),
        None => fallback_pricing.map(|p| (model_id, p)),
    }
}

fn endpoint_pricing(endpoint: &Endpoint) -> Option<ModelPricing> {
    Some(ModelPricing {
        input_cost_per_token: Some(parse_price(&endpoint.pricing.prompt)?),
        output_cost_per_token: Some(parse_price(&endpoint.pricing.completion)?),
        cache_read_input_token_cost: endpoint
            .pricing
            .input_cache_read
            .as_deref()
            .and_then(parse_price),
        cache_creation_input_token_cost: endpoint
            .pricing
            .input_cache_write
            .as_deref()
            .and_then(parse_price),
        ..Default::default()
    })
}

fn quotes_same_base_price(candidate: &ModelPricing, listed: &ModelPricing) -> bool {
    let same = |candidate: Option<f64>, listed: Option<f64>| match (candidate, listed) {
        (Some(candidate), Some(listed)) => (candidate - listed).abs() <= listed.abs() * 1e-9,
        _ => false,
    };

    same(candidate.input_cost_per_token, listed.input_cost_per_token)
        && same(
            candidate.output_cost_per_token,
            listed.output_cost_per_token,
        )
}

/// Pick the pricing row for a model from its OpenRouter endpoints.
///
/// The model author's own endpoint still wins, so `glm-4.7` keeps Z.AI's
/// price rather than a reseller's markup. When the model has no endpoint from
/// its author, the listed price is used exactly as before — but it is taken
/// from an endpoint that quotes that same base price, so the cache rates
/// OpenRouter publishes alongside it survive.
///
/// Discarding them is what broke `tokscope submit`: OpenRouter serves
/// `openai/gpt-5.2-codex` only from an `Azure` endpoint, so the author lookup
/// missed and the row lost the `input_cache_read` price it publishes.
/// Submission validation treats a populated bucket with no rate as
/// unpriceable, so every Codex session — which always carries cached tokens —
/// aborted the whole submission (#1013).
fn select_endpoint_pricing(
    endpoints: &[Endpoint],
    author_name: &str,
    listed: Option<&ModelPricing>,
) -> Option<ModelPricing> {
    if let Some(author) = endpoints.iter().find(|e| e.provider_name == author_name) {
        return endpoint_pricing(author);
    }

    let listed = listed?;
    let matching: Vec<ModelPricing> = endpoints
        .iter()
        .filter_map(endpoint_pricing)
        .filter(|pricing| quotes_same_base_price(pricing, listed))
        .collect();

    // Cache read and cache write are independent fields, so the endpoint
    // publishing the most of them is the one that leaves the fewest buckets
    // unpriceable. On an equal count, retain cache-read pricing: it is the
    // bucket required by Codex usage and must not be lost to an earlier
    // write-only endpoint.
    matching.into_iter().reduce(|best, candidate| {
        if published_cache_rates(&candidate) > published_cache_rates(&best)
            || (published_cache_rates(&candidate) == published_cache_rates(&best)
                && candidate.cache_read_input_token_cost.is_some()
                && best.cache_read_input_token_cost.is_none())
        {
            candidate
        } else {
            best
        }
    })
}

fn published_cache_rates(pricing: &ModelPricing) -> usize {
    usize::from(pricing.cache_read_input_token_cost.is_some())
        + usize::from(pricing.cache_creation_input_token_cost.is_some())
}

/// Fetch all models and get author pricing for each
pub async fn fetch_all_models() -> Result<HashMap<String, ModelPricing>, String> {
    fetch_all_models_from_api_base(API_BASE, true).await
}

async fn fetch_all_models_from_api_base(
    api_base: &str,
    use_disk_cache: bool,
) -> Result<HashMap<String, ModelPricing>, String> {
    if use_disk_cache {
        if let Some(cached) = load_cached() {
            return Ok(cached);
        }
    }

    let api_base = Arc::new(api_base.to_string());
    let models_url = format!("{api_base}/models");
    let client = Arc::new(fetch::pricing_client()?);
    let response = fetch::get_with_retry(&client, &models_url, "OpenRouter").await?;
    let data: ModelsListResponse = response.json().await.map_err(|error| {
        format!(
            "OpenRouter models JSON parse failed: {}",
            describe_error(&error)
        )
    })?;
    let models_with_fallback: Vec<(String, Option<ModelPricing>)> = data
        .data
        .into_iter()
        .map(|m| {
            let fallback = m.pricing.and_then(|p| {
                let input = parse_price(&p.prompt)?;
                let output = parse_price(&p.completion)?;
                Some(ModelPricing {
                    input_cost_per_token: Some(input),
                    output_cost_per_token: Some(output),
                    cache_read_input_token_cost: None,
                    cache_creation_input_token_cost: None,
                    ..Default::default()
                })
            });
            (m.id, fallback)
        })
        .collect();

    if models_with_fallback.is_empty() {
        return Err("OpenRouter returned no models".to_string());
    }

    let models_with_authors: Vec<(String, Option<ModelPricing>)> = models_with_fallback
        .into_iter()
        .filter(|(id, _)| get_author_provider_name(id).is_some())
        .collect();

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));

    let mut handles = Vec::with_capacity(models_with_authors.len());

    for (model_id, fallback) in models_with_authors {
        let client = Arc::clone(&client);
        let api_base = Arc::clone(&api_base);
        let sem = Arc::clone(&semaphore);

        let handle = tokio::spawn(async move {
            fetch_author_pricing(client, api_base, model_id, sem, fallback).await
        });

        handles.push(handle);
    }

    // Collect results
    let mut result = HashMap::new();

    for handle in handles {
        if let Ok(Some((model_id, pricing))) = handle.await {
            result.insert(model_id, pricing);
        }
    }

    // `use_disk_cache` gates the write as well as the read above. See
    // `litellm::fetch_inner` for why the caller's opt-out, not a
    // `TOKSCOPE_CONFIG_DIR` redirect in each test, is what keeps a fixture
    // fetch out of the user's real cache.
    if use_disk_cache && !result.is_empty() {
        if let Err(e) = cache::save_cache(CACHE_FILENAME, &result) {
            eprintln!(
                "[toks] Warning: Failed to cache OpenRouter pricing at {}: {}",
                cache::get_cache_path(CACHE_FILENAME).display(),
                e
            );
        }
    }

    if result.is_empty() {
        return Err("OpenRouter returned no usable pricing rows".to_string());
    }

    Ok(result)
}

pub async fn fetch_all_mapped() -> Result<HashMap<String, ModelPricing>, String> {
    fetch_all_models().await
}

#[cfg(test)]
#[path = "openrouter_tests.rs"]
mod tests;
