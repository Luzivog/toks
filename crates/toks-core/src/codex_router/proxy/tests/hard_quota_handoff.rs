use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::routing::any;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::*;
use crate::rotation::ThreadId;

#[tokio::test]
async fn delivered_hard_limit_releases_ownership_and_queues_one_continuation() {
    let upstream_requests = Arc::new(AtomicUsize::new(0));
    let upstream = Router::new().fallback(any({
        let upstream_requests = upstream_requests.clone();
        move |ws| partial_then_duplicate_limit(ws, upstream_requests.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new_worker(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut socket = connect(&proxy, "hard-limit").await;

    socket
        .send(response_frame("hard-limit").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["delta"], "partial");
    assert_eq!(next_json(&mut socket).await["type"], "turn.failed");
    let thread = ThreadId::new("hard-limit");
    assert_eq!(harness.runtime.waiting_threads().len(), 1);
    assert_eq!(
        harness
            .runtime
            .engine
            .eligible_account_for_thread(&thread)
            .unwrap(),
        Some(AccountId::new("b"))
    );
    let end = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .expect("the exhausted-account bridge must close");
    assert!(match end {
        None | Some(Err(_)) => true,
        Some(Ok(message)) => message.is_close(),
    });

    harness.runtime.reconcile_owned_connections().unwrap();
    assert_eq!(
        harness
            .runtime
            .engine
            .eligible_account_for_thread(&thread)
            .unwrap(),
        Some(AccountId::new("b"))
    );
    let waiting = harness.runtime.waiting_threads();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].thread_id, thread);
    assert_eq!(upstream_requests.load(Ordering::SeqCst), 1);
}

async fn partial_then_duplicate_limit(
    ws: WebSocketUpgrade,
    requests: Arc<AtomicUsize>,
) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        while socket.next().await.is_some() {
            requests.fetch_add(1, Ordering::SeqCst);
            socket
                .send(Message::Text(
                    json!({"type":"response.output_text.delta","delta":"partial"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let limit = Message::Text(usage_error().into());
            socket.send(limit.clone()).await.unwrap();
            let _ = socket.send(limit).await;
        }
    })
}

async fn connect(
    origin: &str,
    thread: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let ws = origin.replacen("http://", "ws://", 1);
    let mut request = format!("{ws}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-a".parse().unwrap());
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
