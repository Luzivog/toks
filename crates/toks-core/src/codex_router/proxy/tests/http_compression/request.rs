use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::to_bytes;
use axum::extract::Request;
use axum::http::header::{CONTENT_ENCODING, CONTENT_LENGTH};

use super::*;

#[tokio::test]
async fn codex_0149_zstd_http_fallback_uses_fast_without_changing_the_wire_encoding() {
    let received = Arc::new(Mutex::new(None));
    let captured = received.clone();
    let upstream = Router::new().fallback(any(move |request: Request| {
        let captured = captured.clone();
        async move {
            let encoding = request.headers()[CONTENT_ENCODING]
                .to_str()
                .unwrap()
                .to_owned();
            let content_length = request.headers()[CONTENT_LENGTH]
                .to_str()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let body = to_bytes(request.into_body(), 128 * 1024 * 1024)
                .await
                .unwrap();
            *captured.lock().unwrap() = Some((encoding, content_length, body));
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let thread = crate::rotation::ThreadId::new("compressed-fast");
    harness
        .runtime
        .engine
        .select_for_thread(Some(&thread), &std::collections::BTreeSet::new())
        .await
        .unwrap()
        .unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = send_zstd(
        &proxy,
        "compressed-fast",
        codex_0149_body("compressed-fast"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let (encoding, content_length, body) = received.lock().unwrap().take().unwrap();
    assert_eq!(encoding, "zstd");
    assert_eq!(content_length, body.len());
    let decoded = zstd::stream::decode_all(Cursor::new(body)).unwrap();
    let request: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(request["service_tier"], "priority");
    assert_eq!(request["client_metadata"]["thread_id"], "compressed-fast");
}

#[tokio::test]
async fn unsupported_or_ambiguous_content_encoding_is_rejected_before_upstream() {
    for encoding in ["gzip", "zstd, identity", "identity, zstd"] {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = counting_upstream(calls.clone());
        let origin = spawn(upstream).await;
        let harness = Harness::new(&[("a", "token-a")]);
        let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

        let response = send_encoded(&proxy, "thread", encoding, b"{}".to_vec()).await;

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn malformed_zstd_is_rejected_before_upstream() {
    let calls = Arc::new(AtomicUsize::new(0));
    let upstream = counting_upstream(calls.clone());
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = send_encoded(&proxy, "thread", "zstd", b"not-zstd".to_vec()).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn zstd_decompressed_body_limit_is_enforced_before_upstream() {
    let calls = Arc::new(AtomicUsize::new(0));
    let upstream = counting_upstream(calls.clone());
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let oversized = vec![b' '; 128 * 1024 * 1024 + 1];

    let response = send_zstd(&proxy, "thread", oversized).await;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn zstd_header_body_thread_conflict_is_rejected_before_upstream() {
    let calls = Arc::new(AtomicUsize::new(0));
    let upstream = counting_upstream(calls.clone());
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = send_zstd(&proxy, "header-thread", codex_0149_body("body-thread")).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn zstd_duplicate_type_is_rejected_before_upstream() {
    for payload in [
        r#"{"type":"response.create","type":"response.cancel","client_metadata":{"thread_id":"thread"}}"#,
        r#"{"type":"response.cancel","type":"response.create","client_metadata":{"thread_id":"thread"}}"#,
        r#"{"type":"response.create","type":"response.create","client_metadata":{"thread_id":"thread"}}"#,
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = counting_upstream(calls.clone());
        let origin = spawn(upstream).await;
        let harness = Harness::new(&[("a", "token-a")]);
        let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

        let response = send_zstd(&proxy, "thread", payload.as_bytes().to_vec()).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

async fn send_zstd(proxy: &str, thread: &str, body: Vec<u8>) -> reqwest::Response {
    let compressed = zstd::stream::encode_all(Cursor::new(body), 3).unwrap();
    send_encoded(proxy, thread, "zstd", compressed).await
}

async fn send_encoded(
    proxy: &str,
    thread: &str,
    encoding: &str,
    body: Vec<u8>,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", thread)
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_ENCODING, encoding)
        .body(body)
        .send()
        .await
        .unwrap()
}

fn counting_upstream(calls: Arc<AtomicUsize>) -> Router {
    Router::new().fallback(any(move || {
        let calls = calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    }))
}
