use super::*;

use super::fixtures::one_percent_snapshot;

mod incident_observability;

#[tokio::test]
async fn fast_limit_retries_standard_on_the_same_account() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream_calls = calls.clone();
    let upstream =
        Router::new().fallback(any(move |headers: HeaderMap, body: axum::body::Bytes| {
            let calls = upstream_calls.clone();
            async move {
                let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
                let frame: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let tier = frame["service_tier"]
                    .as_str()
                    .unwrap_or("default")
                    .to_owned();
                calls.lock().unwrap().push((account, tier.clone()));
                if tier == "priority" {
                    return usage_limit();
                }
                continuing_response()
            }
        }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let client = reqwest::Client::new();
    let mut body = request_body("victim");
    body["service_tier"] = json!("auto");

    let initial = post(&client, &proxy, &body).await;
    assert_eq!(initial.status(), StatusCode::OK);
    initial.text().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    let drained = post(&client, &proxy, &body).await;
    assert_eq!(drained.status(), StatusCode::OK);
    drained.text().await.unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("chatgpt-a".into(), "auto".into()),
            ("chatgpt-a".into(), "priority".into()),
            ("chatgpt-a".into(), "default".into()),
        ]
    );
}

#[tokio::test]
async fn standard_limit_moves_only_the_http_thread_and_keeps_a_websocket_sibling() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().route(
        "/backend-api/codex/responses",
        axum::routing::get(echo_websocket).post(
            move |headers: HeaderMap, body: axum::body::Bytes| {
                let calls = upstream_calls.clone();
                async move {
                    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
                    let frame: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let tier = frame["service_tier"]
                        .as_str()
                        .unwrap_or("default")
                        .to_owned();
                    let call_index = {
                        let mut calls = calls.lock().unwrap();
                        calls.push((account.clone(), tier));
                        calls.len()
                    };
                    if account == "chatgpt-a" && call_index > 1 {
                        usage_limit()
                    } else {
                        continuing_response()
                    }
                }
            },
        ),
    );
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut sibling = connect(&ws, "token-a", "sibling").await;
    let client = reqwest::Client::new();
    let body = request_body("victim");

    let initial = post(&client, &proxy, &body).await;
    assert_eq!(initial.status(), StatusCode::OK);
    initial.text().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    let moved = post(&client, &proxy, &body).await;
    assert_eq!(moved.status(), StatusCode::OK);
    moved.text().await.unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("chatgpt-a".into(), "default".into()),
            ("chatgpt-a".into(), "priority".into()),
            ("chatgpt-a".into(), "default".into()),
            ("chatgpt-b".into(), "default".into()),
        ]
    );

    sibling
        .send(response_frame("sibling").into())
        .await
        .unwrap();
    let sibling_response = next_json(&mut sibling).await;
    assert_eq!(sibling_response["account"], "chatgpt-a");
    assert_eq!(sibling_response["tier"], "priority");
}

#[tokio::test]
async fn a_request_without_a_thread_blocks_only_new_admission() {
    let upstream = Router::new().fallback(any(|headers: HeaderMap| async move {
        if headers["chatgpt-account-id"] == "chatgpt-a" {
            usage_limit()
        } else {
            StatusCode::OK.into_response()
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .json(&json!({"type":"response.create","model":"gpt-5.6-sol"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        harness.runtime.engine.eligible_account().unwrap(),
        Some(AccountId::new("b"))
    );
}

#[tokio::test]
async fn a_coalesced_sse_preamble_prevents_replay_and_the_next_request_uses_standard() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |body: axum::body::Bytes| {
        let calls = upstream_calls.clone();
        async move {
            let frame: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let tier = frame["service_tier"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            calls.lock().unwrap().push(tier.clone());
            if tier == "priority" {
                coalesced_sse_failure()
            } else {
                continuing_response()
            }
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let client = reqwest::Client::new();
    let body = request_body("victim");

    post(&client, &proxy, &body).await.text().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    let failed = post(&client, &proxy, &body).await.text().await.unwrap();
    assert!(failed.contains("response.created"));
    assert!(failed.contains("turn.failed"));

    let next = post(&client, &proxy, &body).await;
    assert!(next.text().await.unwrap().contains("response.completed"));
    assert_eq!(*calls.lock().unwrap(), ["default", "priority", "default"]);
}

#[tokio::test]
async fn concurrent_fast_limits_both_retry_standard_without_blocking_the_thread() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let upstream_calls = calls.clone();
    let upstream_barrier = barrier.clone();
    let upstream = Router::new().fallback(any(move |body: axum::body::Bytes| {
        let calls = upstream_calls.clone();
        let barrier = upstream_barrier.clone();
        async move {
            let frame: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let tier = frame["service_tier"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            calls.lock().unwrap().push(tier.clone());
            if tier == "priority" {
                barrier.wait().await;
                usage_limit()
            } else {
                continuing_response()
            }
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let client = reqwest::Client::new();
    let body = request_body("victim");

    post(&client, &proxy, &body).await.text().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    let (left, right) = tokio::join!(post(&client, &proxy, &body), post(&client, &proxy, &body));
    assert_eq!(left.status(), StatusCode::OK);
    assert_eq!(right.status(), StatusCode::OK);
    left.text().await.unwrap();
    right.text().await.unwrap();

    let calls = calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|tier| *tier == "priority").count(), 2);
    assert_eq!(calls.iter().filter(|tier| *tier == "default").count(), 3);
    assert_eq!(
        harness
            .runtime
            .engine
            .eligible_account_for_thread(&crate::rotation::ThreadId::new("victim"))
            .unwrap(),
        Some(AccountId::new("a"))
    );
}

async fn post(
    client: &reqwest::Client,
    proxy: &str,
    body: &serde_json::Value,
) -> reqwest::Response {
    client
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .json(body)
        .send()
        .await
        .unwrap()
}

fn request_body(thread: &str) -> serde_json::Value {
    json!({
        "type":"response.create",
        "model":"gpt-5.6-sol",
        "service_tier":"default",
        "client_metadata":{"thread_id":thread}
    })
}

fn usage_limit() -> axum::response::Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(json!({"error":{
            "type":"usage_limit_reached",
            "resets_at":2_000_000_000
        }})),
    )
        .into_response()
}

fn continuing_response() -> axum::response::Response {
    (
        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
        concat!(
            "data: {\"type\":\"response.output_item.done\",",
            "\"item\":{\"type\":\"function_call\"}}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{}}\n\n"
        ),
    )
        .into_response()
}

fn split_sse_failure() -> axum::response::Response {
    let frame = format!("data: {}\n\n", usage_error());
    let split = frame.len() / 2;
    let chunks = [frame[..split].to_owned(), frame[split..].to_owned()];
    let body = axum::body::Body::from_stream(futures_util::stream::unfold(
        (chunks.into_iter(), false),
        |(mut chunks, delayed)| async move {
            let chunk = chunks.next()?;
            if delayed {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Some((
                Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(chunk)),
                (chunks, true),
            ))
        },
    ));
    axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
        .body(body)
        .unwrap()
}

fn coalesced_sse_failure() -> axum::response::Response {
    (
        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
        format!(
            "data: {{\"type\":\"response.created\"}}\n\ndata: {}\n\n",
            usage_error()
        ),
    )
        .into_response()
}

async fn echo_websocket(ws: WebSocketUpgrade, headers: HeaderMap) -> impl IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let tier = frame["service_tier"].as_str().unwrap_or("default");
            socket
                .send(Message::Text(
                    json!({"account":account,"tier":tier}).to_string().into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    json!({"type":"response.completed"}).to_string().into(),
                ))
                .await
                .unwrap();
        }
    })
}

async fn connect(
    origin: &str,
    token: &str,
    thread: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!("{origin}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
        .headers_mut()
        .insert("thread-id", thread.parse().unwrap());
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
    serde_json::from_str(&text).unwrap()
}

fn response_frame(thread: &str) -> String {
    json!({
        "type":"response.create",
        "model":"gpt-5.6-sol",
        "service_tier":"default",
        "client_metadata":{"thread_id":thread}
    })
    .to_string()
}
