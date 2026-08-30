use axum::{
    extract::ws::{Message, WebSocketUpgrade},
    response::IntoResponse,
    routing::any,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::{accounts::AccountId, rotation::RotationRuntimeStore};

use super::{app, spawn, Harness};

#[tokio::test]
async fn terminal_error_after_a_tool_call_releases_the_live_response_once() {
    let upstream = Router::new().fallback(any(tool_then_terminal_error));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", "terminal-error").await;

    socket
        .send(response_frame("terminal-error").into())
        .await
        .unwrap();
    assert_eq!(
        next_json(&mut socket).await["type"],
        "response.output_item.done"
    );
    assert_eq!(next_json(&mut socket).await["type"], "turn.failed");
    assert_eq!(next_json(&mut socket).await["type"], "turn.failed");

    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    assert_eq!(runtime.in_flight_count(&AccountId::new("a")), 0);
}

async fn tool_then_terminal_error(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        if socket.next().await.is_none() {
            return;
        }
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
        let failure = Message::Text(
            json!({"type":"turn.failed","error":{"message":"failed"}})
                .to_string()
                .into(),
        );
        socket.send(failure.clone()).await.unwrap();
        socket.send(failure).await.unwrap();
        while socket.next().await.is_some() {}
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
        "client_metadata":{"thread_id":thread}
    })
    .to_string()
}
