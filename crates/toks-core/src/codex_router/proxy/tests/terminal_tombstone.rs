use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::rotation::{ResumeAuthorization, ResumeTerminal, ThreadId, WaitingId, WaitingThread};

const ATTEMPT: &str = "00000000-0000-4000-8000-000000000011";
const REPLACEMENT: &str = "00000000-0000-4000-8000-000000000012";

#[tokio::test]
async fn failure_tombstone_denies_markerless_routes_and_claim_until_atomic_forget() {
    assert_terminal_tombstone(ResumeTerminal::Failure, "failure-tombstone").await;
}

#[tokio::test]
async fn cancelled_tombstone_denies_markerless_routes_and_claim_until_atomic_forget() {
    assert_terminal_tombstone(ResumeTerminal::Cancelled, "cancelled-tombstone").await;
}

async fn assert_terminal_tombstone(terminal: ResumeTerminal, thread_name: &str) {
    let (harness, thread, original) = bound_harness(thread_name);
    let queued = harness
        .runtime
        .finish_resume(
            &original,
            ATTEMPT,
            terminal,
            WaitingId::for_attempt(REPLACEMENT),
        )
        .unwrap()
        .expect("failure and cancellation remain waiting");
    assert!(!harness
        .runtime
        .claim_waiting(&queued, &AccountId::new("a"))
        .unwrap());
    assert_runtime_reloads(&harness, &queued);

    let http_calls = Arc::new(AtomicUsize::new(0));
    let captured = http_calls.clone();
    let http_upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            captured.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    }));
    let websocket_handshakes = Arc::new(AtomicUsize::new(0));
    let captured = websocket_handshakes.clone();
    let websocket_upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        captured.fetch_add(1, Ordering::SeqCst);
        async move { ws.on_upgrade(|_| async {}) }
    }));
    let http_origin = spawn(http_upstream).await;
    let ws_origin = spawn(websocket_upstream)
        .await
        .replacen("http://", "ws://", 1);
    let proxy = spawn(app(harness.state(http_origin, ws_origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-b")
        .json(&frame(&thread))
        .send()
        .await
        .unwrap();
    let mut request = websocket_request(&proxy, &thread);
    request
        .headers_mut()
        .insert("authorization", "Bearer token-b".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(frame(&thread).to_string().into())
        .await
        .unwrap();
    let error = socket.next().await.unwrap().unwrap().into_text().unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(error.contains("unavailable"));
    assert_eq!(http_calls.load(Ordering::SeqCst), 0);
    assert_eq!(websocket_handshakes.load(Ordering::SeqCst), 0);
    assert_eq!(
        harness.runtime.waiting_threads(),
        std::slice::from_ref(&queued)
    );
    assert!(!harness
        .runtime
        .claim_waiting(&queued, &AccountId::new("a"))
        .unwrap());

    harness.runtime.forget_resume(&original, ATTEMPT).unwrap();
    assert_runtime_reloads(&harness, &queued);
    assert!(harness
        .runtime
        .claim_waiting(&queued, &AccountId::new("a"))
        .unwrap());
    assert!(harness.runtime.waiting_threads().is_empty());
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

fn assert_runtime_reloads(harness: &Harness, queued: &WaitingThread) {
    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    assert_eq!(runtime.waiting_threads(), std::slice::from_ref(queued));
}

fn websocket_request(proxy: &str, thread: &ThreadId) -> axum::http::Request<()> {
    let mut request = format!(
        "{}/backend-api/codex/responses",
        proxy.replacen("http://", "ws://", 1)
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("thread-id", thread.as_str().parse().unwrap());
    request
}

fn frame(thread: &ThreadId) -> serde_json::Value {
    json!({"type":"response.create","client_metadata":{"thread_id":thread.as_str()}})
}
