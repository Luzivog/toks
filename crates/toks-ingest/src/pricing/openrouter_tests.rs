use super::*;
use crate::paths::test_env::EnvGuard;
use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use tempfile::TempDir;

fn response_server(status: &'static str, body: &'static str, requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for _ in 0..requests {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "{status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

fn endpoint(
    provider_name: &str,
    prompt: &str,
    completion: &str,
    input_cache_read: Option<&str>,
) -> Endpoint {
    endpoint_with_cache(provider_name, prompt, completion, input_cache_read, None)
}

fn endpoint_with_cache(
    provider_name: &str,
    prompt: &str,
    completion: &str,
    input_cache_read: Option<&str>,
    input_cache_write: Option<&str>,
) -> Endpoint {
    Endpoint {
        provider_name: provider_name.to_string(),
        pricing: EndpointPricing {
            prompt: prompt.to_string(),
            completion: completion.to_string(),
            input_cache_read: input_cache_read.map(str::to_string),
            input_cache_write: input_cache_write.map(str::to_string),
        },
    }
}

fn listed(input: f64, output: f64) -> ModelPricing {
    ModelPricing {
        input_cost_per_token: Some(input),
        output_cost_per_token: Some(output),
        ..Default::default()
    }
}

// Regression: #1013. OpenRouter serves `openai/gpt-5.2-codex` only from an
// `Azure` endpoint, so the `OpenAI` author lookup missed and the row fell
// back to the listed price with its cache rates dropped. Submission
// validation then rejected every Codex session as unpriced.
#[test]
fn cache_rates_survive_when_the_model_has_no_author_endpoint() {
    let endpoints = vec![endpoint(
        "Azure",
        "0.00000175",
        "0.000014",
        Some("0.000000175"),
    )];

    let pricing =
        select_endpoint_pricing(&endpoints, "OpenAI", Some(&listed(1.75e-6, 1.4e-5))).unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(1.75e-6));
    assert_eq!(pricing.output_cost_per_token, Some(1.4e-5));
    assert_eq!(pricing.cache_read_input_token_cost, Some(1.75e-7));
}

// The author's own price stays authoritative, so a reseller endpoint can
// never override it just because it publishes extra cache rates.
#[test]
fn author_endpoint_still_wins_over_other_providers() {
    let endpoints = vec![
        endpoint("Azure", "0.0000035", "0.0000175", Some("0.00000035")),
        endpoint("OpenAI", "0.0000002", "0.0000015", None),
    ];

    let pricing =
        select_endpoint_pricing(&endpoints, "OpenAI", Some(&listed(3.5e-6, 1.75e-5))).unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(2e-7));
    assert_eq!(pricing.output_cost_per_token, Some(1.5e-6));
    assert_eq!(pricing.cache_read_input_token_cost, None);
}

// An endpoint quoting a different base price is a different deal, so its
// cache rate must not be grafted onto the listed price.
#[test]
fn endpoints_quoting_another_base_price_are_not_adopted() {
    let endpoints = vec![endpoint(
        "Azure",
        "0.0000035",
        "0.0000175",
        Some("0.00000035"),
    )];

    assert!(
        select_endpoint_pricing(&endpoints, "OpenAI", Some(&listed(1.75e-6, 1.4e-5))).is_none()
    );
}

// Cache read and cache write are independent fields, so preferring the
// first endpoint that publishes a read rate can hide another endpoint
// that publishes both. Usage with cache-write tokens would then stay
// unpriceable for no reason.
#[test]
fn the_endpoint_publishing_the_most_cache_rates_wins() {
    let endpoints = vec![
        endpoint_with_cache("Azure", "0.00000175", "0.000014", Some("0.000000175"), None),
        endpoint_with_cache(
            "Foundry",
            "0.00000175",
            "0.000014",
            Some("0.000000175"),
            Some("0.0000022"),
        ),
    ];

    let pricing =
        select_endpoint_pricing(&endpoints, "OpenAI", Some(&listed(1.75e-6, 1.4e-5))).unwrap();

    assert_eq!(pricing.cache_read_input_token_cost, Some(1.75e-7));
    assert_eq!(pricing.cache_creation_input_token_cost, Some(2.2e-6));
}

#[test]
fn cache_read_wins_a_cache_rate_count_tie() {
    let endpoints = vec![
        endpoint_with_cache("Azure", "0.00000175", "0.000014", None, Some("0.0000022")),
        endpoint_with_cache(
            "Foundry",
            "0.00000175",
            "0.000014",
            Some("0.000000175"),
            None,
        ),
    ];

    let pricing =
        select_endpoint_pricing(&endpoints, "OpenAI", Some(&listed(1.75e-6, 1.4e-5))).unwrap();

    assert_eq!(pricing.cache_read_input_token_cost, Some(1.75e-7));
    assert_eq!(pricing.cache_creation_input_token_cost, None);
}

#[tokio::test]
async fn list_status_and_decode_failures_remain_explicit() {
    let status = response_server("HTTP/1.1 503 Service Unavailable", "", 3);
    assert!(fetch_all_models_from_api_base(&status, false)
        .await
        .unwrap_err()
        .contains("HTTP 503"));

    let malformed = response_server("HTTP/1.1 200 OK", "not json", 1);
    assert!(fetch_all_models_from_api_base(&malformed, false)
        .await
        .unwrap_err()
        .contains("JSON parse failed"));
}

/// Serve the two request shapes a full OpenRouter fetch makes, dispatching
/// on the path so the author-pricing leg is answered locally instead of
/// reaching openrouter.ai. `response_server` above cannot do this: it
/// replays one fixed body for every connection.
///
/// Bounded to the two requests a single fetch makes so the thread and its
/// listening socket are released when the test ends, rather than parking
/// on `accept` for the life of the test process.
fn openrouter_api_server(models_body: &'static str, endpoints_body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0; 1024];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let body = if request.contains("/endpoints") {
                endpoints_body
            } else {
                models_body
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

/// OpenRouter carried the same defect as LiteLLM and models.dev: the write
/// was gated only on `!result.is_empty()`, never on the caller's opt-out,
/// so a successful fixture fetch overwrote the developer's real
/// `pricing-openrouter.json`. No existing test reached the write — the two
/// cases above both fail before it — which is precisely why the module
/// needs its own proof rather than inheriting confidence from its siblings.
///
/// See the sibling test in `litellm.rs` for why the assertion redirects
/// `TOKSCOPE_CONFIG_DIR` rather than probing the developer's home.
#[tokio::test]
#[serial]
async fn a_fetch_with_caching_disabled_writes_no_cache_file() {
    let temp_config = TempDir::new().unwrap();
    let mut env = EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", temp_config.path());

    let cache_path = cache::get_cache_path(CACHE_FILENAME);
    assert!(
        cache_path.starts_with(temp_config.path()),
        "the config-dir redirect must be in effect or this test proves nothing: {}",
        cache_path.display()
    );

    // `anthropic/` maps to a known author, so this model survives the
    // author filter and drives the endpoints request the fixture answers.
    //
    // The endpoint quotes a different prompt price than the model list on
    // purpose. `fetch_author_pricing` falls back to the listed price on any
    // endpoints failure, so a fixture that quoted the same number on both
    // legs would pass even if the endpoints request had gone to
    // openrouter.ai and failed — which is precisely the regression
    // `API_BASE` exists to prevent. Asserting the endpoint's distinct rate
    // won makes that leg observable.
    let url = openrouter_api_server(
        r#"{"data":[{"id":"anthropic/claude","pricing":{"prompt":"0.000003","completion":"0.000015"}}]}"#,
        r#"{"data":{"id":"anthropic/claude","endpoints":[{"provider_name":"Anthropic","pricing":{"prompt":"0.000009","completion":"0.000015"}}]}}"#,
    );
    let data = fetch_all_models_from_api_base(&url, false)
        .await
        .expect("the fixture serves one priced model");
    assert_eq!(
        data.get("anthropic/claude")
            .and_then(|pricing| pricing.input_cost_per_token),
        Some(9e-6),
        "the local endpoints fixture must have served the author-pricing leg; the listed 3e-6 means it fell back, so this test would no longer catch a hardcoded URL"
    );
    assert_eq!(
        cache::get_cache_path(CACHE_FILENAME),
        cache_path,
        "the redirect moved while the fetch ran, so the assertion below would check a path the fetch never targeted"
    );

    assert!(
        !cache_path.exists(),
        "a fetch that opted out of the cache must not write it, but {} was created",
        cache_path.display()
    );
}
