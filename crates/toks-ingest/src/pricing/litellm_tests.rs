use super::*;
use crate::paths::test_env::EnvGuard;
use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use tempfile::TempDir;

/// Serve one 200 response whose body is well-formed JSON that does not fit
/// `PricingDataset` (a string where an f64 is expected) — the shape an
/// upstream LiteLLM schema change would take.
fn pricing_server(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    });

    url
}

fn malformed_pricing_server() -> String {
    pricing_server(r#"{"some-model":{"input_cost_per_token":"not-a-number"}}"#)
}

/// Serve `MAX_RETRIES` responses with a retryable status, so every attempt
/// is consumed. Mirrors `models_dev::tests::retryable_status_server`.
fn retryable_status_server(status_line: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        for _ in 0..3 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            let response =
                format!("{status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(response.as_bytes());
        }
    });

    url
}

/// A client that cannot outlive a wedged listener thread: without this the
/// tests below block forever instead of failing if `accept` never fires.
fn bounded_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
}

// Regression: retryable statuses never recorded `last_error`, so exhausting
// the retries on 5xx/429 panicked out of `fetch` instead of returning Err.
// That defeated the caller's whole "no single source may be fatal" contract,
// because a panic never reaches the caller at all.
#[tokio::test]
async fn retryable_statuses_return_an_error_rather_than_panicking() {
    let url = retryable_status_server("HTTP/1.1 503 Service Unavailable");

    let result = fetch_inner(&url, false).await;

    assert!(
        result.is_err(),
        "exhausted retries on 503 must surface as Err so the caller can degrade"
    );
}

#[tokio::test]
async fn rate_limit_status_returns_an_error_rather_than_panicking() {
    let url = retryable_status_server("HTTP/1.1 429 Too Many Requests");

    let result = fetch_inner(&url, false).await;

    assert!(result.is_err(), "429 is retried the same way 5xx is");
}

#[tokio::test]
async fn tier_only_rows_are_not_cached_as_usable_pricing() {
    let url = pricing_server(r#"{"tier-only":{"input_cost_per_token_above_272k_tokens":0.00001}}"#);

    let error = fetch_inner(&url, false)
        .await
        .expect_err("a tier rate without a base rate cannot price all tokens");

    assert!(error.contains("no usable pricing rows"));
}

#[tokio::test]
async fn tier_only_rows_are_removed_from_an_otherwise_usable_response() {
    let url = pricing_server(
        r#"{
            "tier-only":{"input_cost_per_token_above_272k_tokens":0.00001},
            "usable":{"input_cost_per_token":0.000005}
        }"#,
    );

    let data = fetch_inner(&url, false)
        .await
        .expect("the response contains one usable base-priced row");

    assert!(!data.contains_key("tier-only"));
    assert!(data.contains_key("usable"));
}

// Pins the mechanism behind #1002: reqwest's Display collapses ANY body
// decode failure to one opaque sentence, so the reported message proves
// only that a response arrived and could not be deserialized — it says
// nothing about TLS, and cannot mean "no connection was made".
//
// Asserted as "Display omits what describe_error recovers" rather than
// against reqwest's and serde_json's exact wording: the wording is upstream
// prose that a dependency bump may reword, and pinning it would redden this
// test without any Toks defect.
#[tokio::test]
async fn reqwest_display_hides_the_decode_cause_that_describe_error_recovers() {
    let url = malformed_pricing_server();
    let error = bounded_client()
        .get(&url)
        .send()
        .await
        .expect("the request itself succeeds")
        .json::<PricingDataset>()
        .await
        .expect_err("the body must fail to deserialize");

    // Anchored on the offending value, which this fixture owns, rather than
    // on reqwest's or serde_json's phrasing, which it does not.
    let displayed = error.to_string();
    assert!(
        !displayed.contains("not-a-number"),
        "Display must say nothing about the payload — that is the bug: {}",
        displayed
    );

    let described = describe_error(&error);
    assert!(
        described.starts_with(&displayed) && described.len() > displayed.len(),
        "describe_error must extend Display with the source chain, got: {}",
        described
    );
    assert!(
        described.contains("not-a-number"),
        "describe_error must surface the serde cause naming the bad value, got: {}",
        described
    );
}

#[test]
fn test_deserialize_model_pricing_with_above_200k_fields() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.0000015,
            "input_cost_per_token_above_200k_tokens": 0.000003,
            "output_cost_per_token": 0.0000075,
            "output_cost_per_token_above_200k_tokens": 0.000015,
            "cache_creation_input_token_cost": 0.000001875,
            "cache_creation_input_token_cost_above_200k_tokens": 0.00000375,
            "cache_read_input_token_cost": 0.00000015,
            "cache_read_input_token_cost_above_200k_tokens": 0.0000003
        }"#,
    )
    .unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(0.0000015));
    assert_eq!(
        pricing.input_cost_per_token_above_200k_tokens,
        Some(0.000003)
    );
    assert_eq!(pricing.output_cost_per_token, Some(0.0000075));
    assert_eq!(
        pricing.output_cost_per_token_above_200k_tokens,
        Some(0.000015)
    );
    assert_eq!(pricing.cache_creation_input_token_cost, Some(0.000001875));
    assert_eq!(
        pricing.cache_creation_input_token_cost_above_200k_tokens,
        Some(0.00000375)
    );
    assert_eq!(pricing.cache_read_input_token_cost, Some(0.00000015));
    assert_eq!(
        pricing.cache_read_input_token_cost_above_200k_tokens,
        Some(0.0000003)
    );
}

#[test]
fn test_deserialize_model_pricing_without_above_200k_fields() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.00000125,
            "output_cost_per_token": 0.00001,
            "cache_creation_input_token_cost": 0.00000125,
            "cache_read_input_token_cost": 0.000000125
        }"#,
    )
    .unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(0.00000125));
    assert_eq!(pricing.input_cost_per_token_above_200k_tokens, None);
    assert_eq!(pricing.output_cost_per_token, Some(0.00001));
    assert_eq!(pricing.output_cost_per_token_above_200k_tokens, None);
    assert_eq!(pricing.cache_creation_input_token_cost, Some(0.00000125));
    assert_eq!(
        pricing.cache_creation_input_token_cost_above_200k_tokens,
        None
    );
    assert_eq!(pricing.cache_read_input_token_cost, Some(0.000000125));
    assert_eq!(pricing.cache_read_input_token_cost_above_200k_tokens, None);
}

#[test]
fn test_deserialize_model_pricing_with_above_272k_fields() {
    let pricing: ModelPricing = serde_json::from_str(
        r#"{
            "input_cost_per_token": 0.000005,
            "input_cost_per_token_above_272k_tokens": 0.000010,
            "output_cost_per_token": 0.000030,
            "output_cost_per_token_above_272k_tokens": 0.000045,
            "cache_read_input_token_cost": 0.0000005,
            "cache_read_input_token_cost_above_272k_tokens": 0.000001
        }"#,
    )
    .unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(0.000005));
    assert_eq!(
        pricing.input_cost_per_token_above_272k_tokens,
        Some(0.000010)
    );
    assert_eq!(pricing.output_cost_per_token, Some(0.000030));
    assert_eq!(
        pricing.output_cost_per_token_above_272k_tokens,
        Some(0.000045)
    );
    assert_eq!(pricing.cache_read_input_token_cost, Some(0.0000005));
    assert_eq!(
        pricing.cache_read_input_token_cost_above_272k_tokens,
        Some(0.000001)
    );
}

/// `use_disk_cache` used to gate only the cache READ while the write ran
/// unconditionally, so every fixture-server test in this module wrote its
/// two-row fixture over the developer's real
/// `~/.config/tokscope/cache/pricing-litellm.json`, evicting the genuine
/// multi-thousand-model LiteLLM dataset for a full TTL. A clobbered cache
/// contributes nothing to pricing lookups, which is exactly the spurious
/// "pricing is unavailable for submitted token usage" submit failure
/// reported in #1021 and #1035 — `cargo test` was manufacturing the bug.
///
/// The assertion redirects `TOKSCOPE_CONFIG_DIR` at a `TempDir` instead of
/// probing the developer's home. `cache::get_cache_path` resolves through
/// `paths::get_config_dir()` either way, so a write that would have landed
/// in the real cache lands in the temp dir here: observable without the
/// test depending on — or risking — whatever the developer's home contains.
/// The `starts_with` assertion is what makes that substitution honest; if
/// the redirect ever stopped taking effect the test would otherwise pass
/// vacuously while the real cache was still being overwritten.
///
/// `#[serial]` is load-bearing for the same reason. `TOKSCOPE_CONFIG_DIR`
/// is process-global, so a concurrent test that restores its own snapshot
/// of it clears this redirect mid-run; the path captured at the top then
/// stops being the path the code would write to, and the final assertion
/// checks an empty temp dir while the real cache is clobbered. The
/// `assert_eq!` after the fetch catches that breach directly instead of
/// letting it read as a pass.
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

    let url = pricing_server(r#"{"usable":{"input_cost_per_token":0.000005}}"#);
    let data = fetch_inner(&url, false)
        .await
        .expect("the fixture serves one usable base-priced row");
    assert!(data.contains_key("usable"), "the fetch itself must succeed");

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
