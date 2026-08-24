use super::*;

use super::fixtures::one_percent_snapshot;

#[tokio::test]
async fn fast_limit_retries_standard_without_disturbing_a_sibling() {
    let upstream = Router::new().fallback(any(fast_limit_for_victim));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut victim = connect(&ws, "token-a", Some("victim")).await;
    let mut victim_duplicate = connect(&ws, "token-a", Some("victim")).await;
    let mut sibling = connect(&ws, "token-a", Some("sibling")).await;
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    victim
        .send(response_frame("victim", "gpt-5.6-sol", "auto").into())
        .await
        .unwrap();
    let victim_response = next_json(&mut victim).await;
    assert_eq!(victim_response["account"], "chatgpt-a");
    assert_eq!(victim_response["tier"], "default");
    assert_eq!(next_json(&mut victim).await["type"], "response.completed");

    victim_duplicate
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let duplicate_response = next_json(&mut victim_duplicate).await;
    assert_eq!(duplicate_response["account"], "chatgpt-a");
    assert_eq!(duplicate_response["tier"], "default");
    assert_eq!(
        next_json(&mut victim_duplicate).await["type"],
        "response.completed"
    );

    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    sibling
        .send(response_frame("sibling", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let sibling_response = next_json(&mut sibling).await;
    assert_eq!(sibling_response["account"], "chatgpt-a");
    assert_eq!(sibling_response["tier"], "priority");
}

#[tokio::test]
async fn standard_limit_moves_only_the_thread_that_received_it() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws, headers| {
        blocked_victim(ws, headers, upstream_calls.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut victim = connect(&ws, "token-a", Some("victim")).await;
    let mut sibling = connect(&ws, "token-a", Some("sibling")).await;
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    victim
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let retry = victim.next().await.unwrap().unwrap().into_text().unwrap();
    assert_eq!(retry, RETRY_FRAME);
    drop(victim);

    let mut moved = connect(&ws, "token-a", Some("victim")).await;
    moved
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let moved_response = next_json(&mut moved).await;
    assert_eq!(moved_response["account"], "chatgpt-b");
    assert_eq!(moved_response["tier"], "default");
    assert_eq!(next_json(&mut moved).await["type"], "response.completed");

    sibling
        .send(response_frame("sibling", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let sibling_response = next_json(&mut sibling).await;
    assert_eq!(sibling_response["account"], "chatgpt-a");
    assert_eq!(sibling_response["tier"], "priority");

    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("chatgpt-a".into(), "victim".into(), "priority".into()),
            ("chatgpt-a".into(), "victim".into(), "default".into()),
            ("chatgpt-b".into(), "victim".into(), "default".into()),
            ("chatgpt-a".into(), "sibling".into(), "priority".into()),
        ]
    );
}

#[tokio::test]
async fn visible_fast_failure_is_not_replayed_and_the_next_turn_uses_standard() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws, headers| {
        visible_fast_limit(ws, headers, upstream_calls.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", Some("victim")).await;
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    socket
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["delta"], "partial");
    let error = next_json(&mut socket).await;
    assert_eq!(error["type"], "turn.failed");
    assert_eq!(*calls.lock().unwrap(), ["priority"]);

    socket
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let retry = next_json(&mut socket).await;
    assert_eq!(retry["account"], "chatgpt-a");
    assert_eq!(retry["tier"], "default");
}

#[tokio::test]
async fn client_requested_fast_is_not_silently_downgraded() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws, headers| {
        blocked_victim(ws, headers, upstream_calls.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", Some("victim")).await;
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    socket
        .send(response_frame("victim", "gpt-5.6-sol", "priority").into())
        .await
        .unwrap();
    let retry = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert_eq!(retry, RETRY_FRAME);
    assert_eq!(
        *calls.lock().unwrap(),
        [("chatgpt-a".into(), "victim".into(), "priority".into())]
    );
}

#[tokio::test]
async fn fast_fallback_survives_an_upstream_disconnect() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws| {
        disconnect_after_fast_limit(ws, upstream_calls.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut first = connect(&ws, "token-a", Some("victim")).await;
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    first
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while first.next().await.is_some() {}
    })
    .await
    .unwrap();

    let mut reconnected = connect(&ws, "token-a", Some("victim")).await;
    reconnected
        .send(response_frame("victim", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let response = next_json(&mut reconnected).await;
    assert_eq!(response["account"], "chatgpt-a");
    assert_eq!(response["tier"], "default");
    assert_eq!(calls.lock().unwrap().last().unwrap(), "default");
}

async fn fast_limit_for_victim(ws: WebSocketUpgrade, headers: HeaderMap) -> impl IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let thread = frame["client_metadata"]["thread_id"].as_str().unwrap();
            let tier = frame["service_tier"].as_str().unwrap_or("default");
            if thread == "victim" && tier == "priority" {
                socket
                    .send(Message::Text(usage_error().into()))
                    .await
                    .unwrap();
                continue;
            }
            socket
                .send(Message::Text(
                    json!({"account":account,"thread":thread,"tier":tier})
                        .to_string()
                        .into(),
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

async fn blocked_victim(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    calls: Arc<Mutex<Vec<(String, String, String)>>>,
) -> impl IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let thread = frame["client_metadata"]["thread_id"]
                .as_str()
                .unwrap()
                .to_owned();
            let tier = frame["service_tier"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            calls
                .lock()
                .unwrap()
                .push((account.clone(), thread.clone(), tier.clone()));
            if account == "chatgpt-a" && thread == "victim" {
                socket
                    .send(Message::Text(usage_error().into()))
                    .await
                    .unwrap();
                continue;
            }
            socket
                .send(Message::Text(
                    json!({"account":account,"thread":thread,"tier":tier})
                        .to_string()
                        .into(),
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

async fn visible_fast_limit(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    calls: Arc<Mutex<Vec<String>>>,
) -> impl IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let tier = frame["service_tier"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            calls.lock().unwrap().push(tier.clone());
            if tier == "priority" {
                socket
                    .send(Message::Text(
                        json!({"type":"response.output_text.delta","delta":"partial"})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                socket
                    .send(Message::Text(usage_error().into()))
                    .await
                    .unwrap();
                continue;
            }
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

async fn disconnect_after_fast_limit(
    ws: WebSocketUpgrade,
    calls: Arc<Mutex<Vec<String>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        let Some(Ok(Message::Text(text))) = socket.next().await else {
            return;
        };
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
        let tier = frame["service_tier"]
            .as_str()
            .unwrap_or("default")
            .to_owned();
        calls.lock().unwrap().push(tier.clone());
        if tier == "priority" {
            socket
                .send(Message::Text(usage_error().into()))
                .await
                .unwrap();
            return;
        }
        socket
            .send(Message::Text(
                json!({"account":"chatgpt-a","tier":tier})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    })
}

async fn connect(
    origin: &str,
    token: &str,
    thread: Option<&str>,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!("{origin}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    if let Some(thread) = thread {
        request
            .headers_mut()
            .insert("thread-id", thread.parse().unwrap());
    }
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

fn response_frame(thread: &str, model: &str, tier: &str) -> String {
    json!({
        "type":"response.create",
        "model":model,
        "service_tier":tier,
        "client_metadata":{"thread_id":thread}
    })
    .to_string()
}
