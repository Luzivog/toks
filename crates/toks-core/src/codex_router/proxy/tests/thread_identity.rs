use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::protocol::{ClientRequestFrame, ThreadIdentity, BAD_THREAD_FRAME};
use super::*;
use crate::rotation::{ResumeAuthorization, ThreadId};
use tokio_tungstenite::tungstenite::Error as WsError;

const ATTEMPT: &str = "123e4567-e89b-42d3-a456-426614174000";

#[test]
fn duplicate_body_metadata_cannot_hide_a_conflicting_thread_identity() {
    for payload in [
        br#"{"client_metadata":{"thread_id":"thread-a","thread_id":"thread-b"}}"#.as_slice(),
        br#"{"client_metadata":{"thread_id":"thread-a"},"client_metadata":{"thread_id":"thread-b"}}"#.as_slice(),
    ] {
        assert_eq!(ThreadIdentity::from_payload(payload), ThreadIdentity::Denied);
    }
}

#[test]
fn duplicate_type_keys_are_denied_by_the_same_parser_that_extracts_identity() {
    for payload in [
        br#"{"type":"response.create","type":"response.cancel","client_metadata":{"thread_id":"thread"}}"#.as_slice(),
        br#"{"type":"response.cancel","type":"response.create","client_metadata":{"thread_id":"thread"}}"#.as_slice(),
        br#"{"type":"response.create","type":"response.create","client_metadata":{"thread_id":"thread"}}"#.as_slice(),
    ] {
        assert_eq!(
            ClientRequestFrame::from_payload(payload),
            ClientRequestFrame::Denied
        );
        assert_eq!(ThreadIdentity::from_payload(payload), ThreadIdentity::Denied);
    }
}

#[tokio::test]
async fn websocket_duplicate_type_never_reaches_upstream_model_traffic() {
    for payload in [
        r#"{"type":"response.create","type":"response.cancel","client_metadata":{"thread_id":"thread"}}"#,
        r#"{"type":"response.cancel","type":"response.create","client_metadata":{"thread_id":"thread"}}"#,
    ] {
        let handshakes = Arc::new(AtomicUsize::new(0));
        let model_frames = Arc::new(AtomicUsize::new(0));
        let upstream = websocket_upstream(handshakes.clone(), model_frames.clone());
        let origin = spawn(upstream).await;
        let harness = Harness::new(&[("a", "token-a")]);
        let proxy = spawn(app(
            harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
        ))
        .await;
        let (mut socket, _) =
            tokio_tungstenite::connect_async(websocket_request(&proxy, "token-a"))
                .await
                .unwrap();

        socket.send(payload.into()).await.unwrap();
        let error = socket.next().await.unwrap().unwrap().into_text().unwrap();
        while socket.next().await.is_some() {}

        assert_eq!(error, BAD_THREAD_FRAME);
        assert_eq!(handshakes.load(Ordering::SeqCst), 1);
        assert_eq!(model_frames.load(Ordering::SeqCst), 0);
        assert!(harness.runtime.waiting_threads().is_empty());
    }
}

#[tokio::test]
async fn http_rejects_conflicting_thread_headers_and_body_before_routing() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            captured.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let header_conflict = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", "thread-a")
        .header("x-thread-id", "thread-b")
        .json(&frame("thread-a"))
        .send()
        .await
        .unwrap();
    let body_conflict = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", "thread-a")
        .json(&frame("thread-b"))
        .send()
        .await
        .unwrap();

    assert_eq!(header_conflict.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_conflict.status(), StatusCode::BAD_REQUEST);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn websocket_rejects_conflicting_headers_before_upstream_connect() {
    let handshakes = Arc::new(AtomicUsize::new(0));
    let upstream = websocket_upstream(handshakes.clone(), Arc::new(AtomicUsize::new(0)));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = websocket_request(&proxy, "token-a");
    request
        .headers_mut()
        .insert("thread-id", "thread-a".parse().unwrap());
    request
        .headers_mut()
        .insert("x-thread-id", "thread-b".parse().unwrap());

    let error = tokio_tungstenite::connect_async(request).await.unwrap_err();

    assert!(
        matches!(error, WsError::Http(response) if response.status() == StatusCode::BAD_REQUEST)
    );
    assert_eq!(handshakes.load(Ordering::SeqCst), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn websocket_rejects_header_body_conflict_without_model_traffic_or_residue() {
    let handshakes = Arc::new(AtomicUsize::new(0));
    let model_frames = Arc::new(AtomicUsize::new(0));
    let upstream = websocket_upstream(handshakes.clone(), model_frames.clone());
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = websocket_request(&proxy, "token-a");
    request
        .headers_mut()
        .insert("thread-id", "thread-a".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    socket
        .send(frame("thread-b").to_string().into())
        .await
        .unwrap();
    let error = socket.next().await.unwrap().unwrap().into_text().unwrap();
    while socket.next().await.is_some() {}

    assert_eq!(error, BAD_THREAD_FRAME);
    assert_eq!(handshakes.load(Ordering::SeqCst), 1);
    assert_eq!(model_frames.load(Ordering::SeqCst), 0);
    assert_eq!(active_threads(&harness, "a"), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn websocket_rejects_overlapping_response_create_before_the_prior_terminal() {
    for second_thread in ["thread-a", "thread-b"] {
        let handshakes = Arc::new(AtomicUsize::new(0));
        let model_frames = Arc::new(AtomicUsize::new(0));
        let upstream = websocket_upstream(handshakes.clone(), model_frames.clone());
        let origin = spawn(upstream).await;
        let harness = Harness::new(&[("a", "token-a")]);
        let proxy = spawn(app(
            harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
        ))
        .await;
        let (mut socket, _) =
            tokio_tungstenite::connect_async(websocket_request(&proxy, "token-a"))
                .await
                .unwrap();

        socket
            .send(frame("thread-a").to_string().into())
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while model_frames.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("upstream must observe the first response.create");
        socket
            .send(frame(second_thread).to_string().into())
            .await
            .unwrap();
        let error = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
            .await
            .expect("overlapping response.create must be rejected promptly")
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        while socket.next().await.is_some() {}

        assert_eq!(error, BAD_THREAD_FRAME);
        assert_eq!(handshakes.load(Ordering::SeqCst), 1);
        assert_eq!(model_frames.load(Ordering::SeqCst), 1);
        assert_eq!(active_threads(&harness, "a"), 0);
        assert!(harness.runtime.waiting_threads().is_empty());
    }
}

#[tokio::test]
async fn exact_resume_marker_cannot_override_a_conflicting_body_identity() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            captured.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let (harness, waiting) = bound_harness("thread-a");
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", "thread-a")
        .header("x-toks-resume-attempt", ATTEMPT)
        .json(&frame("thread-b"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
    harness
        .runtime
        .finish_resume(
            &waiting,
            ATTEMPT,
            crate::rotation::ResumeTerminal::Success,
            crate::rotation::WaitingId::for_attempt(ATTEMPT),
        )
        .unwrap();
}

#[tokio::test]
async fn websocket_exact_resume_marker_rejects_a_different_frame_identity() {
    let handshakes = Arc::new(AtomicUsize::new(0));
    let model_frames = Arc::new(AtomicUsize::new(0));
    let upstream = websocket_upstream(handshakes.clone(), model_frames.clone());
    let origin = spawn(upstream).await;
    let (harness, _waiting) = bound_harness("thread-a");
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = websocket_request(&proxy, "token-a");
    request
        .headers_mut()
        .insert("thread-id", "thread-a".parse().unwrap());
    request
        .headers_mut()
        .insert("x-toks-resume-attempt", ATTEMPT.parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    socket
        .send(frame("thread-b").to_string().into())
        .await
        .unwrap();
    let error = socket.next().await.unwrap().unwrap().into_text().unwrap();
    while socket.next().await.is_some() {}

    assert_eq!(error, BAD_THREAD_FRAME);
    assert_eq!(handshakes.load(Ordering::SeqCst), 1);
    assert_eq!(model_frames.load(Ordering::SeqCst), 0);
    assert_eq!(active_threads(&harness, "a"), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn distinct_session_headers_do_not_conflict_with_thread_identity_over_http() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            captured.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("thread-id", "same-thread")
        .header("x-thread-id", "same-thread")
        .header("session-id", "different-session")
        .header("x-session-id", "different-realtime-session")
        .json(&frame("same-thread"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn websocket_accepts_distinct_session_and_thread_headers() {
    let upstream = Router::new().fallback(any(|ws: WebSocketUpgrade| async move {
        ws.on_upgrade(|mut socket| async move {
            let _ = socket.next().await;
            let _ = socket
                .send(Message::Text(
                    json!({"type":"response.completed"}).to_string().into(),
                ))
                .await;
        })
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let mut request = websocket_request(&proxy, "token-a");
    for name in ["thread-id", "x-thread-id"] {
        request
            .headers_mut()
            .append(name, "same-thread".parse().unwrap());
    }
    request
        .headers_mut()
        .append("session-id", "different-session".parse().unwrap());
    request.headers_mut().append(
        "x-session-id",
        "different-realtime-session".parse().unwrap(),
    );
    request
        .headers_mut()
        .append("thread-id", "same-thread".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    socket
        .send(frame("same-thread").to_string().into())
        .await
        .unwrap();
    let response = socket.next().await.unwrap().unwrap().into_text().unwrap();

    assert!(response.contains("response.completed"));
}

fn websocket_upstream(handshakes: Arc<AtomicUsize>, frames: Arc<AtomicUsize>) -> Router {
    Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        handshakes.fetch_add(1, Ordering::SeqCst);
        let frames = frames.clone();
        async move {
            ws.on_upgrade(move |mut socket| async move {
                while let Some(Ok(message)) = socket.next().await {
                    if matches!(message, Message::Text(_)) {
                        frames.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
        }
    }))
}

fn websocket_request(proxy: &str, token: &str) -> axum::http::Request<()> {
    let mut request = format!(
        "{}/backend-api/codex/responses",
        proxy.replacen("http://", "ws://", 1)
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

fn bound_harness(thread: &str) -> (Harness, crate::rotation::WaitingThread) {
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
    (harness, waiting)
}

fn active_threads(harness: &Harness, account: &str) -> u32 {
    RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap()
        .active_threads(&AccountId::new(account))
}

fn frame(thread: &str) -> serde_json::Value {
    json!({"type":"response.create","client_metadata":{"thread_id":thread}})
}
