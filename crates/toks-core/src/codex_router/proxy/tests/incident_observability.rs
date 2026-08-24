use super::*;

#[test]
fn incident_observability_classifies_usage_blocks() {
    let structured = json!({"error":{"type":"usage_limit_reached","resets_at":2_000_000_000}});
    assert!(usage_block(429, structured.to_string().as_bytes()).is_some());
    assert!(usage_block(500, structured.to_string().as_bytes()).is_none());
    assert!(websocket_usage_block(&structured.to_string()).is_some());

    let error = json!({"type":"error","message":"You've hit your usage limit."});
    assert!(websocket_usage_block(&error.to_string()).is_some());
    let status = json!({
        "type":"response.failed",
        "status":429,
        "error":{"message":"You've hit your usage limit."}
    });
    let incident = websocket_usage_block(&status.to_string())
        .unwrap()
        .incident(
            None,
            None,
            UsageLimitTier::unspecified(),
            UsageLimitPhase::WebSocketFrame,
        );
    assert_eq!(incident.evidence().status(), Some(429));
    assert_eq!(incident.evidence().frame_type(), Some("response.failed"));

    let other = json!({"error":{"type":"rate_limit_reached"}});
    assert!(usage_block(429, other.to_string().as_bytes()).is_none());
    let visible = json!({"type":"response.output_text.delta","delta":"usage limit"});
    assert!(websocket_usage_block(&visible.to_string()).is_none());
}

#[test]
fn incident_observability_redacts_usage_evidence() {
    let payload = json!({"type":"turn.failed","error":{
        "type":"usage_limit_reached",
        "code":"weekly_limit",
        "message":"You've hit your usage limit. Bearer secret-token person@example.test"
    }})
    .to_string();
    let incident = websocket_usage_block(&payload).unwrap().incident(
        Some(crate::rotation::ThreadId::new("thread-safe")),
        Some("gpt-5.6-sol"),
        UsageLimitTier::new(Some("priority"), UsageLimitTierOrigin::Client),
        UsageLimitPhase::WebSocketFrame,
    );
    assert_eq!(
        incident.evidence().classification(),
        UsageLimitClassification::StructuredError
    );
    assert_eq!(incident.evidence().frame_type(), Some("turn.failed"));
    let stored = serde_json::to_string(&incident).unwrap();
    for expected in ["thread-safe", "gpt-5.6-sol", "sha256:"] {
        assert!(stored.contains(expected));
    }
    for secret in ["secret-token", "person@example.test", "You've hit"] {
        assert!(!stored.contains(secret));
    }
}

#[test]
fn incident_observability_uses_an_honest_reroute_frame() {
    let frame: serde_json::Value = serde_json::from_str(RETRY_FRAME).unwrap();
    assert_eq!(frame["status"], 409);
    assert_eq!(frame["error"]["code"], "toks_reconnect_required");
    assert!(frame["error"]["message"]
        .as_str()
        .unwrap()
        .contains("fresh connection"));
    assert!(!RETRY_FRAME.contains("60 minutes"));
}

#[tokio::test]
async fn incident_observability_persists_http_context() {
    let upstream = Router::new().fallback(any(|headers: HeaderMap| async move {
        if headers["authorization"] == "Bearer token-a" {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"error":{"type":"usage_limit_reached"}})),
            )
                .into_response();
        }
        (StatusCode::OK, "account-b").into_response()
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-b")
        .json(&json!({
            "type":"response.create",
            "model":"gpt-5.6-sol",
            "service_tier":"default",
            "client_metadata":{"thread_id":"thread-http"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "account-b");

    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    let incident = usage_incident(&runtime);
    assert_eq!(incident.thread_id().unwrap().as_str(), "thread-http");
    assert_eq!(incident.model(), Some("gpt-5.6-sol"));
    assert_eq!(incident.tier().effective(), Some("default"));
    assert_eq!(incident.tier().origin(), UsageLimitTierOrigin::Client);
    assert_eq!(incident.phase(), UsageLimitPhase::HttpResponse);
    assert_eq!(incident.evidence().status(), Some(429));
}

#[tokio::test]
async fn incident_observability_persists_websocket_handshake_context() {
    let upstream = Router::new().fallback(any(handshake_limit_then_echo));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = format!(
        "{}/backend-api/codex/responses",
        proxy.replacen("http://", "ws://", 1)
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-b".parse().unwrap());
    request
        .headers_mut()
        .insert("thread-id", "handshake-thread".parse().unwrap());
    let (_socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    let incident = usage_incident(&runtime);
    assert_eq!(incident.thread_id().unwrap().as_str(), "handshake-thread");
    assert_eq!(incident.phase(), UsageLimitPhase::WebSocketHandshake);
    assert_eq!(incident.tier().origin(), UsageLimitTierOrigin::Unspecified);
    assert_eq!(incident.evidence().status(), Some(429));
}

#[tokio::test]
async fn incident_observability_persists_websocket_frame_context() {
    let upstream = Router::new().fallback(any(mock_websocket));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let proxy_ws = proxy.replacen("http://", "ws://", 1);
    let mut request = format!("{proxy_ws}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-b".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(
            json!({
                "type":"response.create",
                "model":"gpt-5.6-sol",
                "service_tier":"priority",
                "client_metadata":{"thread_id":"thread-ws-context"}
            })
            .to_string()
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap().into_text().unwrap(),
        RETRY_FRAME
    );

    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    let incident = usage_incident(&runtime);
    assert_eq!(incident.thread_id().unwrap().as_str(), "thread-ws-context");
    assert_eq!(incident.model(), Some("gpt-5.6-sol"));
    assert_eq!(incident.tier().effective(), Some("priority"));
    assert_eq!(incident.tier().origin(), UsageLimitTierOrigin::Client);
    assert_eq!(incident.phase(), UsageLimitPhase::WebSocketFrame);
    assert_eq!(
        incident.evidence().classification(),
        UsageLimitClassification::ErrorMessage
    );
}

fn usage_incident(
    runtime: &crate::rotation::RotationRuntime,
) -> &crate::rotation::UsageLimitIncident {
    runtime
        .events()
        .iter()
        .find_map(|event| match &event.event {
            RotationEventKind::UsageLimited { incident, .. } => Some(incident),
            _ => None,
        })
        .unwrap()
}

async fn handshake_limit_then_echo(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> axum::response::Response {
    if headers["chatgpt-account-id"] == "chatgpt-a" {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(json!({"error":{"type":"usage_limit_reached"}})),
        )
            .into_response();
    }
    ws.on_upgrade(|mut socket| async move { while socket.next().await.is_some() {} })
        .into_response()
}
