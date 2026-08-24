use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::rotation::{
    ResumeAuthorization, ResumeTerminal, ThreadId, UnixMillis, WaitingId, WaitingThread,
};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

const ATTEMPT: &str = "123e4567-e89b-42d3-a456-426614174000";

#[tokio::test]
async fn forgotten_exact_marker_rejects_binary_model_traffic() {
    let (harness, thread, waiting) = bound_harness("forgotten-binary");
    let model_frames = Arc::new(AtomicUsize::new(0));
    let origin = spawn(websocket_upstream(model_frames.clone())).await;
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = websocket_request(&proxy);
    request
        .headers_mut()
        .insert("x-toks-resume-attempt", ATTEMPT.parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    finish_and_forget(&harness, &waiting);
    send_binary_create(&mut socket, &thread).await;
    let error = socket.next().await.unwrap().unwrap().into_text().unwrap();
    while socket.next().await.is_some() {}

    assert_eq!(error, super::super::protocol::BAD_THREAD_FRAME);
    assert_eq!(model_frames.load(Ordering::SeqCst), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn conflicting_binary_thread_is_rejected_without_traffic_or_residue() {
    let model_frames = Arc::new(AtomicUsize::new(0));
    let origin = spawn(websocket_upstream(model_frames.clone())).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = websocket_request(&proxy);
    request
        .headers_mut()
        .insert("thread-id", "header-thread".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    send_binary_create(&mut socket, &ThreadId::new("body-thread")).await;
    let error = socket.next().await.unwrap().unwrap().into_text().unwrap();
    while socket.next().await.is_some() {}

    assert_eq!(error, super::super::protocol::BAD_THREAD_FRAME);
    assert_eq!(model_frames.load(Ordering::SeqCst), 0);
    assert_eq!(active_threads(&harness, "a"), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn upstream_binary_data_is_still_forwarded_to_the_client() {
    let payload = vec![0, 1, 2, 255];
    let expected = payload.clone();
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let payload = payload.clone();
        async move {
            ws.on_upgrade(move |mut socket| async move {
                let _ = socket.send(Message::Binary(payload.into())).await;
            })
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let (mut socket, _) = tokio_tungstenite::connect_async(websocket_request(&proxy))
        .await
        .unwrap();

    let message = socket.next().await.unwrap().unwrap();

    assert_eq!(message.into_data(), expected);
}

#[tokio::test]
async fn unavailable_bridge_rejects_binary_client_data() {
    let harness = Harness::new(&[("a", "token-a")]);
    harness
        .runtime
        .engine
        .block_admission(
            &AccountId::new("a"),
            Some(UnixMillis::new(
                chrono::Utc::now().timestamp_millis() + 60_000,
            )),
        )
        .unwrap();
    let upstream = Router::new().fallback(any(|| async { StatusCode::OK }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let (mut socket, _) = tokio_tungstenite::connect_async(websocket_request(&proxy))
        .await
        .unwrap();

    send_binary_create(&mut socket, &ThreadId::new("binary-unavailable")).await;
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap();

    assert_eq!(error, super::super::protocol::BAD_THREAD_FRAME);
    assert!(harness.runtime.waiting_threads().is_empty());
}

fn websocket_upstream(model_frames: Arc<AtomicUsize>) -> Router {
    Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let model_frames = model_frames.clone();
        async move {
            ws.on_upgrade(move |mut socket| async move {
                while let Some(Ok(message)) = socket.next().await {
                    if matches!(message, Message::Text(_) | Message::Binary(_)) {
                        model_frames.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
        }
    }))
}

fn websocket_request(proxy: &str) -> axum::http::Request<()> {
    let mut request = format!(
        "{}/backend-api/codex/responses",
        proxy.replacen("http://", "ws://", 1)
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-a".parse().unwrap());
    request
}

async fn send_binary_create(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    thread: &ThreadId,
) {
    let payload = json!({
        "type": "response.create",
        "client_metadata": {"thread_id": thread.as_str()}
    });
    socket
        .send(WsMessage::Binary(payload.to_string().into_bytes().into()))
        .await
        .unwrap();
}

fn bound_harness(thread: &str) -> (Harness, ThreadId, WaitingThread) {
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let thread = ThreadId::new(thread);
    harness.runtime.engine.waiting(&thread).unwrap();
    let waiting = harness.runtime.waiting_threads()[0].clone();
    assert_eq!(
        harness
            .runtime
            .authorize_resume(&waiting, ATTEMPT, &AccountId::new("a"))
            .unwrap(),
        ResumeAuthorization::Acquired
    );
    (harness, thread, waiting)
}

fn finish_and_forget(harness: &Harness, waiting: &WaitingThread) {
    harness
        .runtime
        .finish_resume(
            waiting,
            ATTEMPT,
            ResumeTerminal::Success,
            WaitingId::for_attempt(ATTEMPT),
        )
        .unwrap();
    harness.runtime.forget_resume(waiting, ATTEMPT).unwrap();
}

fn active_threads(harness: &Harness, account: &str) -> u32 {
    RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap()
        .active_threads(&AccountId::new(account))
}
