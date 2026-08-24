use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::any;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::*;
use crate::rotation::{ResumeAuthorization, ResumeTerminal, ThreadId, WaitingId, WaitingThread};

const ATTEMPT: &str = "00000000-0000-4000-8000-000000000001";
const WRONG_ATTEMPT: &str = "00000000-0000-4000-8000-000000000002";
const UNKNOWN_ATTEMPT: &str = "00000000-0000-4000-8000-000000000003";
const NONCANONICAL_ATTEMPT: &str = "00000000-0000-4000-8000-0000000000AA";

#[test]
fn marker_must_be_a_canonical_uuid_and_is_never_forwarded() {
    let mut incoming = HeaderMap::new();
    incoming.insert("authorization", "Bearer caller".parse().unwrap());
    incoming.insert("chatgpt-account-id", "caller-account".parse().unwrap());
    incoming.insert("connection", "keep-alive".parse().unwrap());
    incoming.insert("content-length", "7".parse().unwrap());
    incoming.insert("x-codex-test", "kept".parse().unwrap());
    incoming.insert("x-toks-resume-attempt", ATTEMPT.parse().unwrap());
    let outgoing = super::super::headers::upstream_headers(
        &incoming,
        &RouteCredential {
            account_id: AccountId::new("a"),
            access_token: "selected".into(),
            chatgpt_account_id: "selected-account".into(),
        },
        false,
    );
    assert_eq!(outgoing["authorization"], "Bearer selected");
    assert_eq!(outgoing["chatgpt-account-id"], "selected-account");
    assert_eq!(outgoing["x-codex-test"], "kept");
    assert!(!outgoing.contains_key("connection"));
    assert!(!outgoing.contains_key("content-length"));
    assert!(!outgoing.contains_key("x-toks-resume-attempt"));
    assert_eq!(
        super::super::headers::resume_marker(&incoming),
        super::super::headers::ResumeMarker::Canonical(ATTEMPT)
    );

    incoming.insert(
        "x-toks-resume-attempt",
        "00000000-0000-4000-8000-0000000000AA".parse().unwrap(),
    );
    assert_eq!(
        super::super::headers::resume_marker(&incoming),
        super::super::headers::ResumeMarker::Invalid
    );
    incoming.insert(
        "x-toks-resume-attempt",
        "000000000000400080000000000000aa".parse().unwrap(),
    );
    assert_eq!(
        super::super::headers::resume_marker(&incoming),
        super::super::headers::ResumeMarker::Invalid
    );
    incoming.remove("x-toks-resume-attempt");
    incoming.append("x-toks-resume-attempt", ATTEMPT.parse().unwrap());
    incoming.append("x-toks-resume-attempt", WRONG_ATTEMPT.parse().unwrap());
    assert_eq!(
        super::super::headers::resume_marker(&incoming),
        super::super::headers::ResumeMarker::Invalid
    );
}

#[tokio::test]
async fn http_exact_marker_uses_bound_account_after_priority_change_and_strips_marker() {
    let (harness, thread, _) = bound_harness("http-exact");
    prioritize_b(&harness);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let captured = observed.clone();
    let upstream = Router::new().fallback(any(move |headers: HeaderMap| {
        let captured = captured.clone();
        async move {
            captured.lock().unwrap().push((
                headers["chatgpt-account-id"].to_str().unwrap().to_owned(),
                headers.contains_key("x-toks-resume-attempt"),
            ));
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = http_request(&proxy, &thread, Some(ATTEMPT)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(*observed.lock().unwrap(), [("chatgpt-a".into(), false)]);
}

#[tokio::test]
async fn http_wrong_or_missing_marker_is_denied_without_requeue_or_upstream() {
    for marker in [
        Some(WRONG_ATTEMPT),
        Some(UNKNOWN_ATTEMPT),
        Some(NONCANONICAL_ATTEMPT),
        None,
    ] {
        let (harness, thread, _) = bound_harness("http-denied");
        prioritize_b(&harness);
        let calls = Arc::new(Mutex::new(0usize));
        let captured = calls.clone();
        let upstream = Router::new().fallback(any(move || {
            let captured = captured.clone();
            async move {
                *captured.lock().unwrap() += 1;
                StatusCode::OK
            }
        }));
        let origin = spawn(upstream).await;
        let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

        let response = http_request(&proxy, &thread, marker).await;

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(*calls.lock().unwrap(), 0);
        assert!(harness.runtime.waiting_threads().is_empty());
    }
}

#[tokio::test]
async fn websocket_exact_marker_uses_bound_account_and_strips_marker() {
    let (harness, thread, _) = bound_harness("ws-exact");
    prioritize_b(&harness);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let captured = observed.clone();
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade, headers: HeaderMap| {
        let captured = captured.clone();
        async move {
            captured.lock().unwrap().push((
                headers["chatgpt-account-id"].to_str().unwrap().to_owned(),
                headers.contains_key("x-toks-resume-attempt"),
            ));
            ws.on_upgrade(|mut socket| async move {
                let _ = socket.next().await;
                let _ = socket.send(Message::Text("exact".into())).await;
            })
        }
    }));
    let http_origin = spawn(upstream).await;
    let ws_origin = http_origin.replacen("http://", "ws://", 1);
    let proxy = spawn(app(harness.state(http_origin, ws_origin))).await;

    let message = websocket_request(&proxy, &thread, Some(ATTEMPT)).await;

    assert_eq!(message, "exact");
    assert_eq!(*observed.lock().unwrap(), [("chatgpt-a".into(), false)]);
}

#[tokio::test]
async fn websocket_wrong_or_missing_marker_is_denied_without_requeue_or_model_frame() {
    for marker in [
        Some(WRONG_ATTEMPT),
        Some(UNKNOWN_ATTEMPT),
        Some(NONCANONICAL_ATTEMPT),
        None,
    ] {
        let (harness, thread, _) = bound_harness("ws-denied");
        prioritize_b(&harness);
        let calls = Arc::new(Mutex::new((0usize, 0usize)));
        let captured = calls.clone();
        let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
            let captured = captured.clone();
            async move {
                captured.lock().unwrap().0 += 1;
                ws.on_upgrade(move |mut socket| async move {
                    while socket.next().await.is_some() {
                        captured.lock().unwrap().1 += 1;
                    }
                })
            }
        }));
        let http_origin = spawn(upstream).await;
        let ws_origin = http_origin.replacen("http://", "ws://", 1);
        let proxy = spawn(app(harness.state(http_origin, ws_origin))).await;

        let message = websocket_request(&proxy, &thread, marker).await;

        assert!(message.contains("unavailable"), "{marker:?}: {message}");
        let expected = if marker.is_none() { (1, 0) } else { (0, 0) };
        assert_eq!(*calls.lock().unwrap(), expected);
        assert!(harness.runtime.waiting_threads().is_empty());
    }
}

#[tokio::test]
async fn http_threadless_resume_marker_is_denied_without_upstream_or_queue() {
    let (harness, _, _) = bound_harness("http-threadless");
    let frames = Arc::new(Mutex::new(0usize));
    let captured = frames.clone();
    let upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            *captured.lock().unwrap() += 1;
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-b")
        .header("x-toks-resume-attempt", ATTEMPT)
        .json(&json!({"model":"gpt-5.6-sol"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(*frames.lock().unwrap(), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn finished_forgotten_and_stale_markers_are_denied_on_http_and_websocket() {
    let (http, _, _) = bound_harness("wrong-thread-http");
    assert_http_denied(http, ThreadId::new("other-http"), ATTEMPT).await;
    let (websocket, _, _) = bound_harness("wrong-thread-websocket");
    assert_websocket_denied(websocket, ThreadId::new("other-websocket"), ATTEMPT).await;

    for forgotten in [false, true] {
        let (http, thread, waiting) = bound_harness("terminal-http");
        finish(&http, &waiting, forgotten);
        assert_http_denied(http, thread, ATTEMPT).await;

        let (websocket, thread, waiting) = bound_harness("terminal-websocket");
        finish(&websocket, &waiting, forgotten);
        assert_websocket_denied(websocket, thread, ATTEMPT).await;
    }

    let (http, thread, _) = bound_harness("stale-http");
    make_bound_account_unavailable(&http);
    assert_http_denied(http, thread, ATTEMPT).await;
    let (websocket, thread, _) = bound_harness("stale-websocket");
    make_bound_account_unavailable(&websocket);
    assert_websocket_denied(websocket, thread, ATTEMPT).await;
}

#[tokio::test]
async fn http_denial_cannot_requeue_after_legitimate_resume_is_forgotten() {
    let (harness, thread, waiting) = bound_harness("http-race");
    let calls = Arc::new(Mutex::new(0usize));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            *captured.lock().unwrap() += 1;
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let gate = Arc::new(tokio::sync::Barrier::new(2));
    let mut state = harness.state(origin.clone(), origin);
    state.resume_denial_gate = Some(gate.clone());
    let proxy = spawn(app(state)).await;
    let request_thread = thread.clone();
    let request =
        tokio::spawn(
            async move { http_request(&proxy, &request_thread, Some(WRONG_ATTEMPT)).await },
        );

    gate.wait().await;
    finish(&harness, &waiting, true);
    gate.wait().await;
    let response = request.await.unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(*calls.lock().unwrap(), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn threadless_websocket_denial_survives_legitimate_resume_being_forgotten() {
    let (harness, thread, waiting) = bound_harness("websocket-race");
    let calls = Arc::new(Mutex::new((0usize, 0usize)));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let captured = captured.clone();
        async move {
            captured.lock().unwrap().0 += 1;
            ws.on_upgrade(move |mut socket| async move {
                while socket.next().await.is_some() {
                    captured.lock().unwrap().1 += 1;
                }
            })
        }
    }));
    let http_origin = spawn(upstream).await;
    let ws_origin = http_origin.replacen("http://", "ws://", 1);
    let gate = Arc::new(tokio::sync::Barrier::new(2));
    let mut state = harness.state(http_origin, ws_origin);
    state.resume_denial_gate = Some(gate.clone());
    let proxy = spawn(app(state)).await;
    let request_thread = thread.clone();
    let request = tokio::spawn(async move {
        websocket_request(&proxy, &request_thread, Some(WRONG_ATTEMPT)).await
    });

    gate.wait().await;
    finish(&harness, &waiting, true);
    gate.wait().await;
    let message = request.await.unwrap();

    assert!(message.contains("unavailable"));
    assert_eq!(*calls.lock().unwrap(), (0, 0));
    assert!(harness.runtime.waiting_threads().is_empty());
}

#[tokio::test]
async fn threadless_bridge_revalidates_exact_marker_without_requeueing_after_forget() {
    let (harness, thread, waiting) = bound_harness("websocket-bridge-race");
    let calls = Arc::new(Mutex::new((0usize, 0usize)));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let captured = captured.clone();
        async move {
            captured.lock().unwrap().0 += 1;
            ws.on_upgrade(move |mut socket| async move {
                while socket.next().await.is_some() {
                    captured.lock().unwrap().1 += 1;
                }
            })
        }
    }));
    let http_origin = spawn(upstream).await;
    let ws_origin = http_origin.replacen("http://", "ws://", 1);
    let proxy = spawn(app(harness.state(http_origin, ws_origin))).await;
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
        .insert("x-toks-resume-attempt", ATTEMPT.parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    finish(&harness, &waiting, true);
    socket
        .send(
            json!({"type":"response.create","client_metadata":{"thread_id":thread.as_str()}})
                .to_string()
                .into(),
        )
        .await
        .unwrap();
    let message = socket.next().await.unwrap().unwrap().into_text().unwrap();

    assert!(message.contains("unavailable"));
    assert_eq!(*calls.lock().unwrap(), (1, 0));
    assert!(harness.runtime.waiting_threads().is_empty());
}

fn finish(harness: &Harness, waiting: &WaitingThread, forget: bool) {
    harness
        .runtime
        .finish_resume(
            waiting,
            ATTEMPT,
            ResumeTerminal::Success,
            WaitingId::for_attempt(ATTEMPT),
        )
        .unwrap();
    if forget {
        harness.runtime.forget_resume(waiting, ATTEMPT).unwrap();
    }
}

fn make_bound_account_unavailable(harness: &Harness) {
    harness
        .runtime
        .engine
        .block_admission(
            &AccountId::new("a"),
            Some(crate::rotation::UnixMillis::new(
                chrono::Utc::now().timestamp_millis() + 60_000,
            )),
        )
        .unwrap();
}

async fn assert_http_denied(harness: Harness, thread: ThreadId, marker: &str) {
    let calls = Arc::new(Mutex::new(0usize));
    let captured = calls.clone();
    let upstream = Router::new().fallback(any(move || {
        let captured = captured.clone();
        async move {
            *captured.lock().unwrap() += 1;
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let response = http_request(&proxy, &thread, Some(marker)).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(*calls.lock().unwrap(), 0);
    assert!(harness.runtime.waiting_threads().is_empty());
}

async fn assert_websocket_denied(harness: Harness, thread: ThreadId, marker: &str) {
    let frames = Arc::new(Mutex::new(0usize));
    let captured = frames.clone();
    let upstream = Router::new().fallback(any(move |ws: WebSocketUpgrade| {
        let captured = captured.clone();
        async move {
            ws.on_upgrade(move |mut socket| async move {
                while socket.next().await.is_some() {
                    *captured.lock().unwrap() += 1;
                }
            })
        }
    }));
    let http_origin = spawn(upstream).await;
    let ws_origin = http_origin.replacen("http://", "ws://", 1);
    let proxy = spawn(app(harness.state(http_origin, ws_origin))).await;
    let message = websocket_request(&proxy, &thread, Some(marker)).await;
    assert!(message.contains("unavailable"));
    assert_eq!(*frames.lock().unwrap(), 0);
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

fn prioritize_b(harness: &Harness) {
    let store = RotationSettingsStore::for_data_dir(harness._directory.path());
    let mut settings = store.load().unwrap();
    assert!(settings.move_to(&AccountId::new("b"), 0));
    store.save(&settings).unwrap();
}

async fn http_request(proxy: &str, thread: &ThreadId, marker: Option<&str>) -> reqwest::Response {
    let client = reqwest::Client::new();
    let mut request = client
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-b")
        .json(&json!({"client_metadata":{"thread_id":thread.as_str()}}));
    if let Some(marker) = marker {
        request = request.header("x-toks-resume-attempt", marker);
    }
    request.send().await.unwrap()
}

async fn websocket_request(proxy: &str, thread: &ThreadId, marker: Option<&str>) -> String {
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut request = format!("{ws}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-b".parse().unwrap());
    if let Some(marker) = marker {
        request
            .headers_mut()
            .insert("x-toks-resume-attempt", marker.parse().unwrap());
    }
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(
            json!({"type":"response.create","client_metadata":{"thread_id":thread.as_str()}})
                .to_string()
                .into(),
        )
        .await
        .unwrap();
    socket
        .next()
        .await
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap()
        .to_string()
}
