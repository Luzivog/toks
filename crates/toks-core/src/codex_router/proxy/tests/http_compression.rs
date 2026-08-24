use std::io::Cursor;
use std::time::Duration;

use axum::http::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE};

use super::fixtures::one_percent_snapshot;
use super::*;

mod request;

#[tokio::test]
async fn caller_compression_negotiation_cannot_hide_a_forced_fast_limit() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(
        move |headers: HeaderMap, body: axum::body::Bytes| {
            let captured = captured.clone();
            async move {
                let accepted = headers[ACCEPT_ENCODING].to_str().unwrap().to_owned();
                let decoded = zstd::stream::decode_all(Cursor::new(body)).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
                let tier = request["service_tier"].as_str().unwrap().to_owned();
                captured.lock().unwrap().push((accepted, tier.clone()));
                if tier == "priority" {
                    axum::response::Response::builder()
                        .status(StatusCode::TOO_MANY_REQUESTS)
                        .header(CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(
                            json!({"error":{"type":"usage_limit_reached"}}).to_string(),
                        ))
                        .unwrap()
                } else {
                    axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .header(CONTENT_TYPE, "text/event-stream")
                        .body(axum::body::Body::from(concat!(
                            "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\"}}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{}}\n\n"
                        )))
                        .unwrap()
                }
            }
        },
    ));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let thread = crate::rotation::ThreadId::new("compressed-retry");
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

    let response = send_zstd_accepting(
        &proxy,
        "compressed-retry",
        codex_0149_body("compressed-retry"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("identity".into(), "priority".into()),
            ("identity".into(), "default".into()),
        ]
    );
}

#[tokio::test]
async fn identity_response_contract_preserves_standard_limit_failover() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(
        move |headers: HeaderMap, body: axum::body::Bytes| {
            let captured = captured.clone();
            async move {
                let accepted = headers[ACCEPT_ENCODING].to_str().unwrap().to_owned();
                let account = headers["chatgpt-account-id"]
                    .to_str()
                    .unwrap()
                    .to_owned();
                let decoded = zstd::stream::decode_all(Cursor::new(body)).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
                let tier = request["service_tier"].as_str().unwrap().to_owned();
                let call_index = {
                    let mut calls = captured.lock().unwrap();
                    calls.push((accepted, account.clone(), tier));
                    calls.len()
                };
                if account == "chatgpt-a" && call_index > 1 {
                    axum::response::Response::builder()
                        .status(StatusCode::TOO_MANY_REQUESTS)
                        .header(CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(
                            json!({"error":{"type":"usage_limit_reached"}}).to_string(),
                        ))
                        .unwrap()
                } else {
                    axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .header(CONTENT_TYPE, "text/event-stream")
                        .body(axum::body::Body::from(concat!(
                            "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\"}}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{}}\n\n"
                        )))
                        .unwrap()
                }
            }
        },
    ));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let initial = send_zstd_accepting(
        &proxy,
        "compressed-hard-block",
        codex_0149_body("compressed-hard-block"),
    )
    .await;
    assert_eq!(initial.status(), StatusCode::OK);
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    let moved = send_zstd_accepting(
        &proxy,
        "compressed-hard-block",
        codex_0149_body("compressed-hard-block"),
    )
    .await;

    assert_eq!(moved.status(), StatusCode::OK);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("identity".into(), "chatgpt-a".into(), "default".into()),
            ("identity".into(), "chatgpt-a".into(), "priority".into()),
            ("identity".into(), "chatgpt-a".into(), "default".into()),
            ("identity".into(), "chatgpt-b".into(), "default".into()),
        ]
    );
    assert_eq!(
        harness.runtime.eligible_account().unwrap(),
        Some(crate::accounts::AccountId::new("b"))
    );
}

#[tokio::test]
async fn identity_response_contract_streams_and_preserves_follow_up_affinity() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let captured = calls.clone();
    let release_completion = Arc::new(tokio::sync::Notify::new());
    let upstream_release = release_completion.clone();
    let upstream = Router::new().fallback(any(
        move |headers: HeaderMap, body: axum::body::Bytes| {
            let captured = captured.clone();
            let release_completion = upstream_release.clone();
            async move {
                let accepted = headers[ACCEPT_ENCODING].to_str().unwrap().to_owned();
                let account = headers["chatgpt-account-id"]
                    .to_str()
                    .unwrap()
                    .to_owned();
                let decoded = zstd::stream::decode_all(Cursor::new(body)).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
                let tier = request["service_tier"].as_str().unwrap().to_owned();
                let call_index = {
                    let mut calls = captured.lock().unwrap();
                    calls.push((accepted, account, tier));
                    calls.len()
                };
                if call_index == 1 {
                    let stream = futures_util::stream::unfold(0_u8, move |step| {
                        let release_completion = release_completion.clone();
                        async move {
                            match step {
                                0 => Some((
                                    Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(
                                        b"data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\"}}\n\n",
                                    )),
                                    1,
                                )),
                                1 => {
                                    release_completion.notified().await;
                                    Some((
                                        Ok(axum::body::Bytes::from_static(
                                            b"data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
                                        )),
                                        2,
                                    ))
                                }
                                _ => None,
                            }
                        }
                    });
                    axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .header(CONTENT_TYPE, "text/event-stream")
                        .body(axum::body::Body::from_stream(stream))
                        .unwrap()
                } else {
                    axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .body(axum::body::Body::empty())
                        .unwrap()
                }
            }
        },
    ));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = send_zstd_accepting(
        &proxy,
        "compressed-follow-up",
        codex_0149_body("compressed-follow-up"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut response_body = response.bytes_stream();
    let first = tokio::time::timeout(Duration::from_secs(1), response_body.next())
        .await
        .expect("the first SSE event must stream before completion")
        .unwrap()
        .unwrap();
    assert!(first
        .windows(b"response.output_item.done".len())
        .any(|window| window == b"response.output_item.done"));
    release_completion.notify_one();
    let mut remainder = Vec::new();
    while let Some(chunk) = response_body.next().await {
        remainder.extend_from_slice(&chunk.unwrap());
    }
    assert!(remainder
        .windows(b"response.completed".len())
        .any(|window| window == b"response.completed"));
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    let follow_up = send_zstd_accepting(
        &proxy,
        "compressed-follow-up",
        codex_0149_body("compressed-follow-up"),
    )
    .await;

    assert_eq!(follow_up.status(), StatusCode::OK);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("identity".into(), "chatgpt-a".into(), "default".into()),
            ("identity".into(), "chatgpt-a".into(), "priority".into()),
        ]
    );
}

#[tokio::test]
async fn upstream_compression_violation_fails_closed_without_quota_mutation() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move |headers: HeaderMap| {
        let captured = captured.clone();
        async move {
            let accepted = headers[ACCEPT_ENCODING].to_str().unwrap().to_owned();
            let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
            captured.lock().unwrap().push((accepted, account));
            let body = zstd::stream::encode_all(
                Cursor::new(json!({"error":{"type":"usage_limit_reached"}}).to_string()),
                3,
            )
            .unwrap();
            axum::response::Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(CONTENT_TYPE, "application/json")
                .header(CONTENT_ENCODING, "zstd")
                .body(axum::body::Body::from(body))
                .unwrap()
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = send_zstd_accepting(
        &proxy,
        "compressed-violation",
        codex_0149_body("compressed-violation"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        *calls.lock().unwrap(),
        [("identity".into(), "chatgpt-a".into())]
    );
    assert_eq!(
        harness.runtime.eligible_account().unwrap(),
        Some(crate::accounts::AccountId::new("a"))
    );
}

async fn send_zstd_accepting(proxy: &str, thread: &str, body: Vec<u8>) -> reqwest::Response {
    let compressed = zstd::stream::encode_all(Cursor::new(body), 3).unwrap();
    reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", thread)
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_ENCODING, "zstd")
        .header(ACCEPT_ENCODING, "zstd")
        .body(compressed)
        .send()
        .await
        .unwrap()
}

fn codex_0149_body(thread: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model":"gpt-5.6-sol",
        "instructions":"",
        "input":[],
        "tools":[],
        "tool_choice":"auto",
        "parallel_tool_calls":true,
        "reasoning":{"effort":"xhigh"},
        "store":false,
        "stream":true,
        "include":[],
        "service_tier":"default",
        "client_metadata":{
            "session_id":"session",
            "thread_id":thread,
            "turn_id":"turn"
        }
    }))
    .unwrap()
}
