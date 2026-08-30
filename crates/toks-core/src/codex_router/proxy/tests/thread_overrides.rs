use axum::body::Bytes;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::fixtures::one_percent_snapshot;
use super::*;
use crate::rotation::{
    RotationRuntimeStore, RotationSettingsStore, ThreadId, ThreadOverrideChange,
    ThreadRequestSettings,
};
use crate::StoreUpdate;

#[tokio::test]
async fn http_override_rewrites_all_fields_and_records_the_client_request() {
    let captured = Arc::new(Mutex::new(None));
    let upstream_capture = captured.clone();
    let harness = Harness::new(&[("a", "token-a")]);
    let runtime_store = RotationRuntimeStore::for_data_dir(harness._directory.path());
    let upstream = Router::new().fallback(any(move |body: Bytes| {
        let captured = upstream_capture.clone();
        let runtime_store = runtime_store.clone();
        async move {
            let forwarded: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let observed = runtime_store
                .load()
                .unwrap()
                .thread_request_settings(&ThreadId::new("http-override"))
                .unwrap()
                .clone();
            *captured.lock().unwrap() = Some((forwarded, observed));
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let thread = ThreadId::new("http-override");
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.4".into())),
    );
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::ReasoningEffort(Some("high".into())),
    );
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::ServiceTier(Some("default".into())),
    );
    let incoming = json!({
        "model":"client-model",
        "service_tier":"priority",
        "client_metadata":{"thread_id":"http-override"}
    });

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", "http-override")
        .json(&incoming)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let (forwarded, observed) = captured.lock().unwrap().take().unwrap();
    assert_eq!(forwarded["model"], "gpt-5.4");
    assert_eq!(forwarded["reasoning"]["effort"], "high");
    assert_eq!(forwarded["service_tier"], "default");
    assert_eq!(
        observed,
        ThreadRequestSettings {
            model: Some("client-model".into()),
            reasoning_effort: None,
            service_tier: Some("priority".into()),
        }
    );
}

#[tokio::test]
async fn http_fast_eligibility_uses_the_overridden_model() {
    let captured = Arc::new(Mutex::new(None));
    let upstream_capture = captured.clone();
    let upstream = Router::new().fallback(any(move |body: Bytes| {
        let captured = upstream_capture.clone();
        async move {
            *captured.lock().unwrap() = serde_json::from_slice::<serde_json::Value>(&body).ok();
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let thread = ThreadId::new("http-effective-model");
    harness
        .runtime
        .engine
        .select_for_thread(Some(&thread), &Default::default())
        .await
        .unwrap()
        .unwrap();
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.6-sol".into())),
    );
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", "http-effective-model")
        .json(&json!({
            "model":"gpt-5.3-codex-spark",
            "service_tier":"default",
            "client_metadata":{"thread_id":"http-effective-model"}
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let forwarded = captured.lock().unwrap().take().unwrap();
    assert_eq!(forwarded["model"], "gpt-5.6-sol");
    assert_eq!(forwarded["service_tier"], "priority");
}

#[tokio::test]
async fn websocket_overrides_apply_to_each_turn_without_reconnecting() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let upstream_capture = captured.clone();
    let harness = Harness::new(&[("a", "token-a")]);
    let runtime_store = RotationRuntimeStore::for_data_dir(harness._directory.path());
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let captured = upstream_capture.clone();
        let runtime_store = runtime_store.clone();
        async move {
            ws.on_upgrade(move |mut socket| async move {
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    let forwarded: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let observed = runtime_store
                        .load()
                        .unwrap()
                        .thread_request_settings(&ThreadId::new("ws-override"))
                        .unwrap()
                        .clone();
                    captured.lock().unwrap().push((forwarded.clone(), observed));
                    socket
                        .send(Message::Text(json!({"seen":forwarded}).to_string().into()))
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
    }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let thread = ThreadId::new("ws-override");
    let mut socket = connect(
        &proxy.replacen("http://", "ws://", 1),
        "token-a",
        "ws-override",
    )
    .await;

    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.4".into())),
    );
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::ReasoningEffort(Some("high".into())),
    );
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::ServiceTier(Some("default".into())),
    );
    send_frame(&mut socket, "client-model", "low", "priority").await;
    let first = next_json(&mut socket).await["seen"].clone();
    assert_eq!(first["model"], "gpt-5.4");
    assert_eq!(first["reasoning"]["effort"], "high");
    assert_eq!(first["reasoning"]["summary"], "auto");
    assert_eq!(first["service_tier"], "default");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    set_override(&harness, &thread, ThreadOverrideChange::ServiceTier(None));
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.6-sol".into())),
    );
    send_frame(&mut socket, "gpt-5.3-codex-spark", "medium", "default").await;
    let second = next_json(&mut socket).await["seen"].clone();
    assert_eq!(second["model"], "gpt-5.6-sol");
    assert_eq!(second["service_tier"], "priority");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.3-codex-spark".into())),
    );
    send_frame(&mut socket, "gpt-5.6-sol", "xhigh", "default").await;
    let third = next_json(&mut socket).await["seen"].clone();
    assert_eq!(third["model"], "gpt-5.3-codex-spark");
    assert_eq!(third["service_tier"], "default");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    let idle = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    assert_eq!(idle.in_flight_count(&AccountId::new("a")), 0);
    let idle_settings = idle.thread_request_settings(&thread).unwrap();
    assert_eq!(idle_settings.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(idle_settings.reasoning_effort.as_deref(), Some("xhigh"));
    assert_eq!(idle_settings.service_tier.as_deref(), Some("default"));

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[0].1.model.as_deref(), Some("client-model"));
    assert_eq!(captured[0].1.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(captured[0].1.service_tier.as_deref(), Some("priority"));
    assert_eq!(captured[1].1.model.as_deref(), Some("gpt-5.3-codex-spark"));
    assert_eq!(captured[2].1.model.as_deref(), Some("gpt-5.6-sol"));
}

#[tokio::test]
async fn websocket_fast_fallback_keeps_model_and_reasoning_overrides() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let calls = upstream_calls.clone();
        async move {
            ws.on_upgrade(move |mut socket| async move {
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    let forwarded: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let tier = forwarded["service_tier"].as_str().unwrap().to_owned();
                    calls.lock().unwrap().push(forwarded.clone());
                    if tier == "priority" {
                        socket
                            .send(Message::Text(usage_error().into()))
                            .await
                            .unwrap();
                    } else {
                        socket
                            .send(Message::Text(json!({"seen":forwarded}).to_string().into()))
                            .await
                            .unwrap();
                        socket
                            .send(Message::Text(
                                json!({"type":"response.completed"}).to_string().into(),
                            ))
                            .await
                            .unwrap();
                    }
                }
            })
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let thread = ThreadId::new("ws-override");
    let mut socket = connect(
        &proxy.replacen("http://", "ws://", 1),
        "token-a",
        "ws-override",
    )
    .await;
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.6-sol".into())),
    );
    set_override(
        &harness,
        &thread,
        ThreadOverrideChange::ReasoningEffort(Some("ultra".into())),
    );
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    send_frame(&mut socket, "gpt-5.3-codex-spark", "low", "default").await;
    let response = next_json(&mut socket).await["seen"].clone();
    assert_eq!(response["model"], "gpt-5.6-sol");
    assert_eq!(response["reasoning"]["effort"], "ultra");
    assert_eq!(response["service_tier"], "default");
    assert_eq!(next_json(&mut socket).await["type"], "response.completed");

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["service_tier"], "priority");
    assert_eq!(calls[1]["service_tier"], "default");
    for call in calls.iter() {
        assert_eq!(call["model"], "gpt-5.6-sol");
        assert_eq!(call["reasoning"]["effort"], "ultra");
    }
}

fn set_override(harness: &Harness, thread: &ThreadId, change: ThreadOverrideChange) {
    RotationSettingsStore::for_data_dir(harness._directory.path())
        .update(|settings| {
            let changed = settings.set_thread_override(thread, change).unwrap();
            StoreUpdate::from_changed((), changed)
        })
        .unwrap();
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

async fn send_frame(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    model: &str,
    effort: &str,
    tier: &str,
) {
    socket
        .send(
            json!({
                "type":"response.create",
                "model":model,
                "reasoning":{"effort":effort,"summary":"auto"},
                "service_tier":tier,
                "client_metadata":{"thread_id":"ws-override"}
            })
            .to_string()
            .into(),
        )
        .await
        .unwrap();
}

async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
    serde_json::from_str(&text).unwrap()
}
