use super::*;

use super::super::protocol::{requested_model, with_service_tier};
use crate::accounts::ProviderAccount;
use crate::limits::{LimitSnapshot, LimitWindow, Provider};

#[test]
fn service_tier_upgrade_preserves_faster_requests_and_the_rest_of_the_turn() {
    let frame = response_frame("thread", "gpt-5.6-sol", "default");
    assert_eq!(requested_model(&frame).as_deref(), Some("gpt-5.6-sol"));
    let upgraded: serde_json::Value =
        serde_json::from_str(&with_service_tier(&frame, "priority").unwrap()).unwrap();
    assert_eq!(upgraded["service_tier"], "priority");
    assert_eq!(upgraded["client_metadata"]["thread_id"], "thread");

    for tier in ["fast", "priority", "ultrafast"] {
        let original = response_frame("thread", "gpt-5.6-sol", tier);
        assert_eq!(with_service_tier(&original, "priority"), Some(original));
    }
    let other = json!({"type":"response.output_text.delta","delta":"hi"}).to_string();
    assert!(with_service_tier(&other, "priority").is_none());
}

#[tokio::test]
async fn only_a_thread_attached_before_exhaustion_drains_in_place() {
    let upstream = Router::new().fallback(any(echo_turn));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", Some("existing")).await;

    socket
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["account"], "chatgpt-a");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    harness
        .runtime
        .engine
        .apply_snapshots(&[drained_snapshot("a")], chrono::Utc::now())
        .unwrap();
    socket
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let existing = next_json(&mut socket).await;
    assert_eq!(existing["account"], "chatgpt-a");
    assert_eq!(existing["tier"], "priority");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    socket
        .send(response_frame("new", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let retry = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert_eq!(retry, RETRY_FRAME);

    let mut fresh = connect(&ws, "token-a", Some("new")).await;
    fresh
        .send(response_frame("new", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let fresh_response = next_json(&mut fresh).await;
    assert_eq!(fresh_response["account"], "chatgpt-b");
    assert_eq!(fresh_response["tier"], "default");
}

#[tokio::test]
async fn tool_follow_ups_stay_active_until_the_final_response() {
    let upstream = Router::new().fallback(any(tool_then_final));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", Some("tool-thread")).await;

    socket
        .send(response_frame("tool-thread", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(
        next_json(&mut socket).await["type"],
        "response.output_item.done"
    );
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");
    assert_eq!(active_threads(&harness, "a"), 1);

    socket
        .send(response_frame("tool-thread", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");
    assert_eq!(active_threads(&harness, "a"), 0);
}

#[tokio::test]
async fn a_grandfathered_thread_reconnects_to_its_draining_account() {
    let upstream = Router::new().fallback(any(echo_turn));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let first = connect(&ws, "token-a", Some("existing")).await;
    harness
        .runtime
        .engine
        .apply_snapshots(&[drained_snapshot("a")], chrono::Utc::now())
        .unwrap();
    drop(first);

    let mut reconnected = connect(&ws, "token-b", Some("existing")).await;
    reconnected
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let response = next_json(&mut reconnected).await;
    assert_eq!(response["account"], "chatgpt-a");
    assert_eq!(response["tier"], "priority");
}

#[tokio::test]
async fn unsupported_models_stay_standard_and_the_toggle_is_live() {
    let upstream = Router::new().fallback(any(echo_turn));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", Some("existing")).await;
    harness
        .runtime
        .engine
        .apply_snapshots(&[drained_snapshot("a")], chrono::Utc::now())
        .unwrap();

    socket
        .send(response_frame("existing", "gpt-5.3-codex-spark", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["tier"], "default");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    let store = RotationSettingsStore::for_data_dir(harness._directory.path());
    let mut settings = store.load().unwrap();
    settings.set_fast_when_draining(false);
    store.save(&settings).unwrap();
    socket
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let unavailable = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert!(
        unavailable.contains("usage_limit_reached"),
        "got {unavailable}"
    );
}

async fn echo_turn(ws: WebSocketUpgrade, headers: HeaderMap) -> impl IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let tier = frame["service_tier"].as_str().unwrap_or("absent");
            let echoed = json!({"account":account,"tier":tier}).to_string();
            socket.send(Message::Text(echoed.into())).await.unwrap();
            socket
                .send(Message::Text(
                    json!({"type":"response.completed"}).to_string().into(),
                ))
                .await
                .unwrap();
        }
    })
}

async fn tool_then_final(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        let mut response_index = 0;
        while let Some(Ok(Message::Text(_))) = socket.next().await {
            if response_index == 0 {
                socket
                    .send(Message::Text(
                        json!({
                            "type":"response.output_item.done",
                            "item":{"type":"function_call"}
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
            socket
                .send(Message::Text(
                    json!({"type":"response.completed","response":{}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            response_index += 1;
        }
    })
}

fn active_threads(harness: &Harness, account: &str) -> u32 {
    RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap()
        .active_threads(&AccountId::new(account))
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

fn drained_snapshot(id: &str) -> LimitSnapshot {
    LimitSnapshot {
        windows: vec![LimitWindow {
            id: "weekly".into(),
            label: "Weekly".into(),
            percent_used: 100.0,
            resets_at: Some(chrono::Utc::now() + chrono::Duration::hours(3)),
            severity: None,
            scope: None,
            is_active: true,
            raw: json!({}),
        }],
        ..LimitSnapshot::loading_account(
            Provider::Codex,
            ProviderAccount {
                id: AccountId::new(id),
                ..ProviderAccount::unidentified_for(Provider::Codex)
            },
        )
    }
}
