//! Authenticated loopback proxy for local Codex model traffic.

mod engine;
mod headers;
mod http;
mod inbound;
mod lease;
mod protocol;
mod types;
mod websocket;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State, WebSocketUpgrade};
use axum::http::{header, Response, StatusCode, Uri};
use axum::routing::get;
use axum::Router;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, WaitingThread};

use engine::Engine;
use headers::bearer_token;
use inbound::InboundTokens;
use types::{LocalCredentials, SharedCredentials};

const CODEX_PATH: &str = "/backend-api/codex";
const CHATGPT_HTTP: &str = "https://chatgpt.com";
const CHATGPT_WS: &str = "wss://chatgpt.com";
pub(crate) const HEALTH_BODY: &str = "toks-router\n";

#[derive(Clone)]
pub struct RouterRuntimeHandle {
    engine: Arc<Engine>,
    credentials: SharedCredentials,
}

impl RouterRuntimeHandle {
    pub fn discover() -> Result<Self> {
        let credentials: SharedCredentials = Arc::new(LocalCredentials);
        let engine = Engine::discover(credentials.clone())?;
        Ok(Self {
            engine,
            credentials,
        })
    }

    pub fn eligible_account(&self) -> Result<Option<AccountId>> {
        self.engine.eligible_account()
    }

    pub fn waiting_threads(&self) -> Vec<WaitingThread> {
        self.engine.waiting_threads()
    }

    pub fn waiting(&self, thread: &ThreadId) -> Result<()> {
        self.engine.waiting(thread)
    }

    pub fn claim_waiting(&self, thread: &ThreadId, account: &AccountId) -> Result<bool> {
        self.engine.claim_waiting(thread, account)
    }
}

#[derive(Clone)]
struct ProxyState {
    engine: Arc<Engine>,
    tokens: Arc<InboundTokens>,
    http: reqwest::Client,
    upstream: Upstream,
}

impl ProxyState {
    fn new(runtime: &RouterRuntimeHandle) -> Self {
        Self {
            engine: runtime.engine.clone(),
            tokens: Arc::new(InboundTokens::new(runtime.credentials.clone())),
            http: reqwest::Client::new(),
            upstream: Upstream::chatgpt(),
        }
    }
}

#[derive(Clone)]
struct Upstream {
    http_origin: String,
    ws_origin: String,
}

impl Upstream {
    fn chatgpt() -> Self {
        Self {
            http_origin: CHATGPT_HTTP.into(),
            ws_origin: CHATGPT_WS.into(),
        }
    }

    fn http_url(&self, uri: &Uri) -> String {
        format!("{}{}", self.http_origin, path_and_query(uri))
    }

    fn ws_url(&self, uri: &Uri) -> String {
        format!("{}{}", self.ws_origin, path_and_query(uri))
    }
}

pub async fn serve(runtime: RouterRuntimeHandle) -> Result<()> {
    let state = ProxyState::new(&runtime);
    let app = app(state);
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, super::ROUTER_PORT));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("binding the Toks router loopback socket")?;
    tokio::spawn(heartbeat(runtime));
    axum::serve(listener, app)
        .await
        .context("serving Codex traffic")
}

fn app(state: ProxyState) -> Router {
    Router::new()
        .route("/health", get(|| async { HEALTH_BODY }))
        .fallback(dispatch)
        .with_state(state)
}

async fn heartbeat(runtime: RouterRuntimeHandle) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let _ = runtime.engine.heartbeat();
    }
}

async fn dispatch(State(state): State<ProxyState>, request: Request) -> Response<Body> {
    let path = request.uri().path();
    if path != CODEX_PATH && !path.starts_with(&format!("{CODEX_PATH}/")) {
        return empty(StatusCode::NOT_FOUND);
    }
    let Some(token) = bearer_token(request.headers()) else {
        return empty(StatusCode::UNAUTHORIZED);
    };
    if !state.tokens.accepts(token) {
        return empty(StatusCode::UNAUTHORIZED);
    }
    if !is_websocket(&request) {
        return http::forward(state, request).await;
    }
    let (mut parts, _) = request.into_parts();
    let uri = parts.uri.clone();
    let headers = parts.headers.clone();
    match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => websocket::forward(state, upgrade, uri, headers).await,
        Err(_) => empty(StatusCode::BAD_REQUEST),
    }
}

fn is_websocket(request: &Request) -> bool {
    request
        .headers()
        .get(header::UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"))
}

fn path_and_query(uri: &Uri) -> &str {
    uri.path_and_query()
        .map_or(uri.path(), |value| value.as_str())
}

fn empty(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .expect("valid response")
}

#[cfg(test)]
mod tests;
