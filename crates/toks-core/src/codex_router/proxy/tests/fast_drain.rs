use super::*;

use super::fixtures::one_percent_snapshot;
use crate::codex_router::proxy::protocol::{
    requested_model, usage_block, with_service_tier, BAD_THREAD_FRAME,
};
use crate::codex_router::proxy::{
    engine::{AttemptedTier, ResponseDelivery, RouteTier},
    lease::StreamLease,
};

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
async fn selection_reserves_affinity_before_the_drain_snapshot() {
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let thread = crate::rotation::ThreadId::new("threshold-race");
    let selected = harness
        .runtime
        .engine
        .select_for_thread(Some(&thread), &std::collections::BTreeSet::new())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.account_id, AccountId::new("a"));

    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    let lease = StreamLease::open(
        harness.runtime.engine.clone(),
        &AccountId::new("a"),
        &thread,
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(lease.tier(), RouteTier::Fast);
}

#[tokio::test]
async fn a_lease_rejects_a_selection_that_became_stale_during_credentials() {
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let thread = crate::rotation::ThreadId::new("stale-selection");
    let selected = harness
        .runtime
        .engine
        .select_for_thread(Some(&thread), &std::collections::BTreeSet::new())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.account_id, AccountId::new("a"));
    harness
        .runtime
        .engine
        .request_usage_limited(
            &AccountId::new("a"),
            Some(&thread),
            AttemptedTier::Other,
            ResponseDelivery::NothingDelivered,
            Some(crate::rotation::UnixMillis::new(2_000_000_000_000)),
            usage_block(429, br#"{"error":{"type":"usage_limit_reached"}}"#)
                .unwrap()
                .incident(
                    Some(thread.clone()),
                    Some("gpt-5.6-sol"),
                    crate::rotation::UsageLimitTier::new(
                        Some("default"),
                        crate::rotation::UsageLimitTierOrigin::Client,
                    ),
                    crate::rotation::UsageLimitPhase::HttpResponse,
                ),
        )
        .unwrap();

    assert!(StreamLease::open(
        harness.runtime.engine.clone(),
        &AccountId::new("a"),
        &thread,
        None,
    )
    .unwrap()
    .is_none());
    let reselected = harness
        .runtime
        .engine
        .select_for_thread(Some(&thread), &std::collections::BTreeSet::new())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reselected.account_id, AccountId::new("b"));
    harness
        .runtime
        .engine
        .release_reservation(&AccountId::new("b"), &thread)
        .unwrap();
}

#[tokio::test]
async fn only_a_thread_attached_before_the_threshold_drains_in_place() {
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
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
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
    assert_eq!(retry, BAD_THREAD_FRAME);

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
async fn tool_follow_ups_keep_affinity_without_counting_as_live() {
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
    assert_eq!(active_threads(&harness, "a"), 0);

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
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
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
async fn a_body_only_thread_id_cannot_steal_drain_affinity() {
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
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    drop(first);

    let mut headerless = connect(&ws, "token-b", None).await;
    headerless
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(
        headerless
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap(),
        RETRY_FRAME
    );

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
async fn unsupported_models_stay_standard_and_supported_models_use_fast() {
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
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    socket
        .send(response_frame("existing", "gpt-5.3-codex-spark", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["tier"], "default");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    socket
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let response = next_json(&mut socket).await;
    assert_eq!(response["account"], "chatgpt-a");
    assert_eq!(response["tier"], "priority");
}

#[tokio::test]
async fn a_delivered_response_preamble_is_never_replayed() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws| {
        preamble_then_fast_limit(ws, upstream_calls.clone())
    }));
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
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    socket
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["type"], "response.created");
    assert_eq!(next_json(&mut socket).await["type"], "turn.failed");
    assert_eq!(*calls.lock().unwrap(), ["priority"]);

    socket
        .send(response_frame("existing", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["tier"], "default");
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

async fn preamble_then_fast_limit(
    ws: WebSocketUpgrade,
    calls: Arc<Mutex<Vec<String>>>,
) -> impl IntoResponse {
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
                        json!({"type":"response.created"}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                socket
                    .send(Message::Text(usage_error().into()))
                    .await
                    .unwrap();
            } else {
                socket
                    .send(Message::Text(json!({"tier":tier}).to_string().into()))
                    .await
                    .unwrap();
            }
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
