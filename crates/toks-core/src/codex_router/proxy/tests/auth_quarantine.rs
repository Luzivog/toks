use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use tokio_tungstenite::tungstenite::Message as ClientMessage;

#[test]
fn credential_proof_covers_both_bearer_and_provider_account_header() {
    let account = AccountId::new("a");
    let first = RouteCredential {
        account_id: account.clone(),
        access_token: "same-token".into(),
        chatgpt_account_id: "provider-a".into(),
    };
    let corrected = RouteCredential {
        account_id: account,
        access_token: "same-token".into(),
        chatgpt_account_id: "provider-b".into(),
    };

    assert_ne!(first.fingerprint(), corrected.fingerprint());
}

#[tokio::test]
async fn http_unchanged_rejected_token_stays_quarantined_until_exact_account_token_changes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |headers: HeaderMap| {
        let calls = upstream_calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            if headers["authorization"] == "Bearer token-b" {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    unchanged_refresh(&harness);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    for thread in ["first", "unchanged"] {
        let response = post(&proxy, thread).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let account = AccountId::new("a");
    harness.credentials.credentials.lock().unwrap().insert(
        account.clone(),
        RouteCredential {
            account_id: account,
            access_token: "token-b".into(),
            chatgpt_account_id: "chatgpt-a".into(),
        },
    );
    let repaired = post(&proxy, "changed").await;
    assert_eq!(repaired.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn websocket_unchanged_rejected_token_stays_quarantined_until_token_changes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let upstream_calls = calls.clone();
    let upstream = Router::new().route(
        "/backend-api/codex/responses",
        axum::routing::get(move |ws: WebSocketUpgrade, headers: HeaderMap| {
            let calls = upstream_calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                if headers["authorization"] != "Bearer token-b" {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                ws.on_upgrade(|mut socket| async move {
                    if socket.next().await.is_some() {
                        let _ = socket
                            .send(Message::Text(
                                json!({"type":"response.completed"}).to_string().into(),
                            ))
                            .await;
                    }
                })
                .into_response()
            }
        }),
    );
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    unchanged_refresh(&harness);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;

    for thread in ["first", "unchanged"] {
        let mut socket = websocket(&proxy, thread).await;
        socket
            .send(ClientMessage::Text(request(thread).to_string().into()))
            .await
            .unwrap();
        let frame = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(frame.contains("usage_limit_reached"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let account = AccountId::new("a");
    harness.credentials.credentials.lock().unwrap().insert(
        account.clone(),
        RouteCredential {
            account_id: account,
            access_token: "token-b".into(),
            chatgpt_account_id: "chatgpt-a".into(),
        },
    );
    let mut repaired = websocket(&proxy, "changed").await;
    repaired
        .send(ClientMessage::Text(request("changed").to_string().into()))
        .await
        .unwrap();
    assert!(repaired
        .next()
        .await
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap()
        .contains("response.completed"));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

fn unchanged_refresh(harness: &Harness) {
    let account = AccountId::new("a");
    harness.credentials.refreshes.lock().unwrap().insert(
        account.clone(),
        RouteCredential {
            account_id: account,
            access_token: "token-a".into(),
            chatgpt_account_id: "chatgpt-a".into(),
        },
    );
}

async fn post(proxy: &str, thread: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .json(&request(thread))
        .send()
        .await
        .unwrap()
}

async fn websocket(
    proxy: &str,
    thread: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!(
        "{}/backend-api/codex/responses",
        proxy.replacen("http://", "ws://", 1)
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-a".parse().unwrap());
    request.headers_mut().insert(
        "x-codex-thread-id",
        thread.parse().expect("valid thread header"),
    );
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

fn request(thread: &str) -> serde_json::Value {
    json!({
        "type":"response.create",
        "model":"gpt-5.6-sol",
        "client_metadata":{"thread_id":thread}
    })
}
