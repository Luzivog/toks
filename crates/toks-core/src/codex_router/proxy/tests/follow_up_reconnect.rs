use std::sync::atomic::{AtomicUsize, Ordering};

use super::fixtures::one_percent_snapshot;
use super::*;

#[tokio::test]
async fn websocket_tool_follow_up_reconnects_to_the_original_account_after_one_percent() {
    let turns = Arc::new(AtomicUsize::new(0));
    let upstream_closed = Arc::new(tokio::sync::Notify::new());
    let upstream = Router::new().fallback(any({
        let turns = turns.clone();
        let upstream_closed = upstream_closed.clone();
        move |ws, headers| tool_then_final(ws, headers, turns.clone(), upstream_closed.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut first = connect(&ws, "token-a", Some("tool-reconnect")).await;
    first
        .send(response_frame("tool-reconnect", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    assert_eq!(
        next_json(&mut first).await["type"],
        "response.output_item.done"
    );
    assert_eq!(next_json(&mut first).await["type"], "response.completed");
    let disconnected = upstream_closed.notified();
    drop(first);
    tokio::time::timeout(std::time::Duration::from_secs(1), disconnected)
        .await
        .unwrap();

    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    let mut reconnected = connect(&ws, "token-b", Some("tool-reconnect")).await;
    reconnected
        .send(response_frame("tool-reconnect", "gpt-5.6-sol", "default").into())
        .await
        .unwrap();
    let response = next_json(&mut reconnected).await;
    assert_eq!(response["account"], "chatgpt-a");
    assert_eq!(response["tier"], "priority");
    assert_eq!(
        next_json(&mut reconnected).await["type"],
        "response.completed"
    );
}

async fn tool_then_final(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    turns: Arc<AtomicUsize>,
    upstream_closed: Arc<tokio::sync::Notify>,
) -> impl IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: serde_json::Value = serde_json::from_str(&text).unwrap();
            let tier = request["service_tier"].as_str().unwrap_or("default");
            if turns.fetch_add(1, Ordering::SeqCst) == 0 {
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
            } else {
                socket
                    .send(Message::Text(
                        json!({"account":account,"tier":tier}).to_string().into(),
                    ))
                    .await
                    .unwrap();
            }
            socket
                .send(Message::Text(
                    json!({"type":"response.completed"}).to_string().into(),
                ))
                .await
                .unwrap();
        }
        upstream_closed.notify_waiters();
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
