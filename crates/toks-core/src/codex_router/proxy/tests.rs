use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::Router;
use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tempfile::TempDir;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::accounts::AccountId;
use crate::rotation::{RotationRuntimeStore, RotationSettings, RotationSettingsStore};

use super::engine::Engine;
use super::headers::upstream_headers;
use super::protocol::{usage_block, websocket_usage_block, RETRY_FRAME};
use super::types::{CredentialFailure, CredentialSource, RouteCredential, SharedCredentials};
use super::{app, InboundTokens, ProxyState, RouterRuntimeHandle, Upstream};

mod fast_drain;
mod inbound;
mod remote_control;

struct FakeCredentials {
    ids: Vec<AccountId>,
    credentials: Mutex<BTreeMap<AccountId, RouteCredential>>,
    incoming: Mutex<BTreeMap<String, AccountId>>,
    refreshes: Mutex<BTreeMap<AccountId, RouteCredential>>,
}

impl CredentialSource for FakeCredentials {
    fn account_ids(&self) -> Vec<AccountId> {
        self.ids.clone()
    }

    fn incoming_account(&self, token: &str) -> Option<AccountId> {
        self.incoming.lock().unwrap().get(token).cloned()
    }

    fn credential<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move {
            self.credentials
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .ok_or(CredentialFailure::NeedsSignIn)
        })
    }

    fn refresh<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move {
            let refreshed = self
                .refreshes
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .ok_or(CredentialFailure::NeedsSignIn)?;
            self.credentials
                .lock()
                .unwrap()
                .insert(account.clone(), refreshed.clone());
            Ok(refreshed)
        })
    }
}

struct Harness {
    _directory: TempDir,
    credentials: Arc<FakeCredentials>,
    runtime: RouterRuntimeHandle,
}

impl Harness {
    fn new(accounts: &[(&str, &str)]) -> Self {
        Self::new_with_reconciled_settings(accounts, true)
    }

    fn new_with_reconciled_settings(accounts: &[(&str, &str)], reconciled: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let ids = accounts
            .iter()
            .map(|(id, _)| AccountId::new(*id))
            .collect::<Vec<_>>();
        let credentials = Arc::new(FakeCredentials {
            ids: ids.clone(),
            credentials: Mutex::new(
                accounts
                    .iter()
                    .map(|(id, token)| {
                        let account_id = AccountId::new(*id);
                        (
                            account_id.clone(),
                            RouteCredential {
                                account_id,
                                access_token: (*token).into(),
                                chatgpt_account_id: format!("chatgpt-{id}"),
                            },
                        )
                    })
                    .collect(),
            ),
            incoming: Mutex::new(
                accounts
                    .iter()
                    .map(|(id, token)| ((*token).into(), AccountId::new(*id)))
                    .collect(),
            ),
            refreshes: Mutex::new(BTreeMap::new()),
        });
        let settings_store = RotationSettingsStore::for_data_dir(directory.path());
        let mut settings = RotationSettings::default();
        if reconciled {
            settings.reconcile(&ids);
        }
        settings.set_enabled(true);
        settings_store.save(&settings).unwrap();
        let source: SharedCredentials = credentials.clone();
        // A fixed stand-in for the CLI's `models_cache.json` so tier decisions
        // never depend on the developer's own Codex install.
        let catalogue_path = directory.path().join("models_cache.json");
        std::fs::write(
            &catalogue_path,
            r#"{"models":[
                {"slug":"gpt-5.6-sol","service_tiers":[{"id":"priority","name":"Fast"}]},
                {"slug":"gpt-5.3-codex-spark","service_tiers":[]}
            ]}"#,
        )
        .unwrap();
        let engine = Engine::with_catalogue(
            source.clone(),
            settings_store,
            RotationRuntimeStore::for_data_dir(directory.path()),
            super::catalogue::Catalogue::at(Some(catalogue_path)),
        )
        .unwrap();
        Self {
            _directory: directory,
            credentials,
            runtime: RouterRuntimeHandle {
                engine,
                credentials: source,
            },
        }
    }

    fn state(&self, http_origin: String, ws_origin: String) -> ProxyState {
        ProxyState {
            engine: self.runtime.engine.clone(),
            tokens: Arc::new(InboundTokens::new(self.runtime.credentials.clone())),
            http: reqwest::Client::new(),
            upstream: Upstream {
                http_origin,
                ws_origin,
            },
        }
    }
}

#[test]
fn engine_routes_fresh_accounts_without_mutating_ui_settings() {
    let harness = Harness::new_with_reconciled_settings(&[("a", "token-a")], false);
    assert_eq!(
        harness.runtime.eligible_account().unwrap(),
        Some(AccountId::new("a"))
    );
    let settings = RotationSettingsStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    assert!(settings.priority().is_empty());
}

#[test]
fn usage_blocks_match_structured_and_message_frames() {
    // Structured legacy/synthetic shape: still gated on 429 for HTTP.
    let structured = json!({"error":{"type":"usage_limit_reached","resets_at":2_000_000_000}});
    assert!(usage_block(429, structured.to_string().as_bytes()).is_some());
    assert!(usage_block(500, structured.to_string().as_bytes()).is_none());
    assert!(websocket_usage_block(&structured.to_string()).is_some());

    // Real upstream frames: no `status`, no `error.type`, message-based.
    let error_frame = json!({
        "type":"error",
        "message":"You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Jul 20th, 2026 7:53 AM."
    });
    assert!(websocket_usage_block(&error_frame.to_string()).is_some());
    let turn_failed = json!({
        "type":"turn.failed",
        "error":{"message":"You've hit your usage limit. Add credits to continue, or try again at Aug 29, 2026, 6:59 AM."}
    });
    assert!(websocket_usage_block(&turn_failed.to_string()).is_some());

    // A retryable rate limit without the usage marker is left alone.
    let other = json!({"error":{"type":"rate_limit_reached"}});
    assert!(usage_block(429, other.to_string().as_bytes()).is_none());
    // A reconnect notice is an error frame but not a usage limit.
    let reconnect = json!({"type":"error","message":"Reconnecting... 2/5 (401 Unauthorized)"});
    assert!(websocket_usage_block(&reconnect.to_string()).is_none());
    // Normal streamed model text that merely mentions a usage limit is not a block.
    let visible = json!({"type":"response.output_text.delta","delta":"your usage limit is 100"});
    assert!(websocket_usage_block(&visible.to_string()).is_none());
}

#[test]
fn upstream_headers_replace_identity_and_drop_hop_headers() {
    let mut incoming = HeaderMap::new();
    incoming.insert("authorization", "Bearer caller".parse().unwrap());
    incoming.insert("chatgpt-account-id", "caller-account".parse().unwrap());
    incoming.insert("connection", "keep-alive".parse().unwrap());
    incoming.insert("x-codex-test", "kept".parse().unwrap());
    let account = AccountId::new("a");
    let outgoing = upstream_headers(
        &incoming,
        &RouteCredential {
            account_id: account,
            access_token: "selected".into(),
            chatgpt_account_id: "selected-account".into(),
        },
        false,
    );
    assert_eq!(outgoing["authorization"], "Bearer selected");
    assert_eq!(outgoing["chatgpt-account-id"], "selected-account");
    assert_eq!(outgoing["x-codex-test"], "kept");
    assert!(!outgoing.contains_key("connection"));
}

#[tokio::test]
async fn http_rotates_only_for_an_exact_usage_block() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |headers: HeaderMap| {
        let calls = upstream_calls.clone();
        async move {
            let auth = headers["authorization"].to_str().unwrap().to_string();
            calls.lock().unwrap().push(auth.clone());
            if auth == "Bearer token-a" {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    axum::Json(json!({
                        "error":{"type":"usage_limit_reached","resets_at":2_000_000_000}
                    })),
                )
                    .into_response()
            } else {
                (StatusCode::OK, "account-b").into_response()
            }
        }
    }));
    let upstream_origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy = spawn(app(harness.state(upstream_origin.clone(), upstream_origin))).await;
    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-b")
        .json(&json!({"client_metadata":{"thread_id":"thread-http"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), "account-b");
    assert_eq!(*calls.lock().unwrap(), ["Bearer token-a", "Bearer token-b"]);
}

#[tokio::test]
async fn refreshed_clients_can_reconnect_with_their_startup_token() {
    let harness = Harness::new(&[("a", "old-token")]);
    let account = AccountId::new("a");
    harness.credentials.refreshes.lock().unwrap().insert(
        account.clone(),
        RouteCredential {
            account_id: account,
            access_token: "new-token".into(),
            chatgpt_account_id: "chatgpt-a".into(),
        },
    );
    let upstream = Router::new().fallback(any(|headers: HeaderMap| async move {
        if headers["authorization"] == "Bearer old-token" {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        }
    }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    for _ in 0..2 {
        let response = reqwest::Client::new()
            .post(format!("{proxy}/backend-api/codex/responses"))
            .bearer_auth("old-token")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn websocket_reconnects_before_output_and_preserves_partial_output() {
    let upstream = Router::new().fallback(any(mock_websocket));
    let upstream_origin = spawn(upstream).await;
    let ws_origin = upstream_origin.replacen("http://", "ws://", 1);
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let proxy_origin = spawn(app(harness.state(upstream_origin, ws_origin))).await;
    let proxy_ws = proxy_origin.replacen("http://", "ws://", 1);
    let first = websocket(&proxy_ws, "token-b", "thread-ws").await;
    assert_eq!(first, RETRY_FRAME);
    let second = websocket(&proxy_ws, "token-b", "thread-ws").await;
    assert!(second.contains("response.output_text.delta"));

    let partial_harness = Harness::new(&[("partial", "token-partial")]);
    let upstream = Router::new().fallback(any(mock_websocket));
    let http_origin = spawn(upstream).await;
    let ws_origin = http_origin.replacen("http://", "ws://", 1);
    let proxy = spawn(app(partial_harness.state(http_origin, ws_origin))).await;
    let proxy_ws = proxy.replacen("http://", "ws://", 1);
    let mut request = format!("{proxy_ws}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-partial".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(response_create("thread-partial"))
        .await
        .unwrap();
    let delta = socket.next().await.unwrap().unwrap().into_text().unwrap();
    let error = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert!(delta.contains("response.output_text.delta"));
    assert!(error.contains("usage limit"));
    assert_ne!(error, RETRY_FRAME);
    assert_eq!(
        partial_harness.runtime.waiting_threads()[0]
            .thread_id
            .as_str(),
        "thread-partial"
    );
}

#[tokio::test]
async fn idle_websocket_fails_back_when_higher_priority_account_resets() {
    let upstream = Router::new().fallback(any(mock_websocket));
    let upstream_origin = spawn(upstream).await;
    let ws_origin = upstream_origin.replacen("http://", "ws://", 1);
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    harness
        .runtime
        .engine
        .block(
            &AccountId::new("a"),
            Some(crate::rotation::UnixMillis::new(
                chrono::Utc::now().timestamp_millis() + 50,
            )),
        )
        .unwrap();
    let proxy_origin = spawn(app(harness.state(upstream_origin, ws_origin))).await;
    let proxy_ws = proxy_origin.replacen("http://", "ws://", 1);
    let mut request = format!("{proxy_ws}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer token-b".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(75)).await;
    socket
        .send(response_create("thread-failback"))
        .await
        .unwrap();
    let response = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert_eq!(response, RETRY_FRAME);
}

async fn mock_websocket(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> impl axum::response::IntoResponse {
    let account = headers["chatgpt-account-id"].to_str().unwrap().to_string();
    ws.on_upgrade(move |mut socket| async move {
        let _ = socket.next().await;
        if account == "chatgpt-b" {
            let _ = socket
                .send(Message::Text(
                    json!({"type":"response.output_text.delta","delta":"ok"})
                        .to_string()
                        .into(),
                ))
                .await;
        } else if account == "chatgpt-partial" {
            let _ = socket
                .send(Message::Text(
                    json!({"type":"response.output_text.delta","delta":"half"})
                        .to_string()
                        .into(),
                ))
                .await;
            let _ = socket.send(Message::Text(usage_error().into())).await;
        } else {
            let _ = socket.send(Message::Text(usage_error().into())).await;
        }
    })
}

async fn websocket(origin: &str, token: &str, thread: &str) -> String {
    let mut request = format!("{origin}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket.send(response_create(thread)).await.unwrap();
    socket
        .next()
        .await
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap()
        .to_string()
}

fn response_create(thread: &str) -> tokio_tungstenite::tungstenite::Message {
    json!({"type":"response.create","client_metadata":{"thread_id":thread}})
        .to_string()
        .into()
}

fn usage_error() -> String {
    // The real upstream shape: no `status`/`error.type`, reset only in prose.
    json!({"type":"turn.failed","error":{
        "message":"You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Jul 20th, 2026 7:53 AM."
    }})
    .to_string()
}

async fn spawn(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}
