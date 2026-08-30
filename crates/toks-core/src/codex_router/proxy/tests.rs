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
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{
    RotationEventKind, RotationRuntimeStore, RotationSettings, RotationSettingsStore,
    UsageLimitClassification, UsageLimitPhase, UsageLimitTier, UsageLimitTierOrigin,
    WorkerConnectionOwner,
};

use super::engine::{Engine, EngineConfig};
use super::protocol::{usage_block, websocket_usage_block, RETRY_FRAME};
use super::types::{CredentialFailure, CredentialSource, RouteCredential, SharedCredentials};
use super::{app, InboundTokens, ProxyState, RouterRuntimeHandle, Upstream};

mod account_activation;
mod auth_quarantine;
mod auth_refresh;
mod binary_frames;
mod control_frames;
mod fast_drain;
mod fast_failover;
mod fixtures;
mod follow_up_reconnect;
mod handoff_connection;
mod hard_quota_handoff;
mod http_compression;
mod http_failover;
mod inbound;
mod incident_observability;
mod lifecycle_cleanup;
mod remote_control;
mod resume_boundary;
mod terminal_tombstone;
mod thread_identity;
mod thread_overrides;

struct FakeCredentials {
    ids: Vec<AccountId>,
    credentials: Mutex<BTreeMap<AccountId, RouteCredential>>,
    incoming: Mutex<BTreeMap<String, AccountId>>,
    refreshes: Mutex<BTreeMap<AccountId, RouteCredential>>,
    refresh_gate: Mutex<
        Option<(
            tokio::sync::mpsc::UnboundedSender<()>,
            Arc<tokio::sync::Notify>,
        )>,
    >,
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
            let gate = self.refresh_gate.lock().unwrap().clone();
            if let Some((started, proceed)) = gate {
                let _ = started.send(());
                proceed.notified().await;
            }
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
        Self::new_with_owner(accounts, reconciled, None)
    }

    fn new_worker(accounts: &[(&str, &str)]) -> Self {
        Self::new_with_owner(
            accounts,
            true,
            Some(WorkerConnectionOwner::new(1, 1).expect("nonzero worker identity")),
        )
    }

    fn new_with_owner(
        accounts: &[(&str, &str)],
        reconciled: bool,
        connection_owner: Option<WorkerConnectionOwner>,
    ) -> Self {
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
            refresh_gate: Mutex::new(None),
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
        let engine = Engine::new(EngineConfig {
            credentials: source.clone(),
            settings: settings_store,
            runtime_store: RotationRuntimeStore::for_data_dir(directory.path()),
            catalogue: super::catalogue::Catalogue::at(Some(catalogue_path)),
            connection_owner,
            thread_sources: ThreadSourceStore::discover(),
        })
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
            lifetime: super::ConnectionLifetime::new(|| {}),
            resume_denial_gate: None,
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

#[tokio::test]
async fn reset_acknowledgement_is_not_exposed_over_http() {
    let harness = Harness::new(&[("a", "token-a")]);
    let upstream = Router::new().fallback(any(|| async { StatusCode::OK }));
    let origin = spawn(upstream).await;
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/banked-reset-consumed"))
        .json(&json!({"accountId":"a"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
        .block_admission(
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
