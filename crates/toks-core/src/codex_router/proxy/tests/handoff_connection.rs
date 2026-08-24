use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::{spawn, Harness};
use crate::codex_router::proxy::connection::serve_state_connection;
use crate::codex_router::proxy::{
    serve_connection, ConnectionLifetime, ConnectionService, InboundTokens,
};

#[tokio::test]
async fn handed_off_connection_serves_health_and_finishes_cleanly() {
    let harness = Harness::new(&[("a", "token-a")]);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let client = tokio::spawn(async move {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        response
    });
    let (server, _) = listener.accept().await.unwrap();

    serve_connection(harness.runtime.clone(), server)
        .await
        .unwrap();
    let response = client.await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    assert!(response.ends_with(crate::codex_router::proxy::HEALTH_BODY.as_bytes()));
}

#[tokio::test]
async fn websocket_upgrade_keeps_worker_lifetime_until_the_bridge_closes() {
    let upstream = axum::Router::new().fallback(axum::routing::any(
        |ws: axum::extract::WebSocketUpgrade| async move {
            ws.on_upgrade(|mut socket| async move {
                while let Some(Ok(message)) = socket.next().await {
                    if socket.send(message).await.is_err() {
                        break;
                    }
                }
            })
        },
    ));
    let upstream_origin = spawn(upstream).await;
    let ws_origin = upstream_origin.replacen("http://", "ws://", 1);
    let harness = Harness::new(&[("a", "token-a")]);
    let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let closed_for_guard = closed.clone();
    let lifetime = ConnectionLifetime::new(move || {
        closed_for_guard.store(true, std::sync::atomic::Ordering::Release);
    });
    let mut state = harness.state(upstream_origin, ws_origin);
    state.lifetime = lifetime;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        serve_state_connection(state, stream).await.unwrap();
    });
    let mut request = format!("ws://{address}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-a".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket.send("ping".into()).await.unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap().into_text().unwrap(),
        "ping"
    );
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert!(!closed.load(std::sync::atomic::Ordering::Acquire));

    socket.close(None).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !closed.load(std::sync::atomic::Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn worker_connection_service_shares_ephemeral_admissions_between_connections() {
    let upstream = axum::Router::new().fallback(axum::routing::any(|| async { "ok" }));
    let upstream_origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "opaque-startup-token")]);
    let mut state = harness.state(upstream_origin.clone(), upstream_origin);
    state.tokens = std::sync::Arc::new(InboundTokens::at(
        harness.runtime.credentials.clone(),
        harness._directory.path().join("worker-admissions.json"),
    ));
    let service = ConnectionService::from_state(state);

    assert_eq!(request_once(&service, "opaque-startup-token").await, 200);
    harness
        .credentials
        .incoming
        .lock()
        .unwrap()
        .remove("opaque-startup-token");
    assert_eq!(request_once(&service, "opaque-startup-token").await, 200);
}

async fn request_once(service: &ConnectionService, token: &str) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let service = service.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        service
            .serve(stream, ConnectionLifetime::new(|| {}))
            .await
            .unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://{address}/backend-api/codex/responses"))
        .header("connection", "close")
        .bearer_auth(token)
        .body("{}")
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    drop(response);
    server.await.unwrap();
    status
}
