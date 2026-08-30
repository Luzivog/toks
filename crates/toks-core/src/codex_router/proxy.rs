//! Authenticated loopback proxy for local Codex model traffic.

mod catalogue;
#[cfg(test)]
mod catalogue_tests;
mod connection;
mod engine;
mod headers;
mod heartbeat;
mod http;
mod inbound;
mod lease;
mod protocol;
mod routing;
mod runtime_handle;
mod types;
mod websocket;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::http::Uri;

#[cfg(test)]
use connection::serve_connection;
pub(crate) use connection::{ConnectionLifetime, ConnectionService};
use engine::Engine;
pub(crate) use heartbeat::heartbeat;
use inbound::InboundTokens;
use routing::app;
use types::SharedCredentials;

const CODEX_PATH: &str = "/backend-api/codex";
const CHATGPT_HTTP: &str = "https://chatgpt.com";
const CHATGPT_WS: &str = "wss://chatgpt.com";
pub(crate) const HEALTH_BODY: &str = "toks-router\n";
/// Reserved owner generation for the supported single-process router mode.
/// Restartable systemd workers must never use this generation.
pub(crate) const DIRECT_ROUTER_GENERATION: u64 = u64::MAX;

#[derive(Clone)]
pub struct RouterRuntimeHandle {
    engine: Arc<Engine>,
    credentials: SharedCredentials,
}

#[derive(Clone)]
struct ProxyState {
    engine: Arc<Engine>,
    tokens: Arc<InboundTokens>,
    http: reqwest::Client,
    upstream: Upstream,
    lifetime: ConnectionLifetime,
    #[cfg(test)]
    resume_denial_gate: Option<Arc<tokio::sync::Barrier>>,
}

impl ProxyState {
    fn new(runtime: &RouterRuntimeHandle) -> Self {
        ConnectionService::new(runtime).state(ConnectionLifetime::new(|| {}))
    }

    async fn pause_after_resume_denial(&self) {
        #[cfg(test)]
        if let Some(gate) = &self.resume_denial_gate {
            gate.wait().await;
            gate.wait().await;
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

pub(crate) async fn bind_listener() -> Result<tokio::net::TcpListener> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, super::ROUTER_PORT));
    tokio::net::TcpListener::bind(address)
        .await
        .context("binding the Toks router loopback socket")
}

pub async fn serve(listener: tokio::net::TcpListener, runtime: RouterRuntimeHandle) -> Result<()> {
    let state = ProxyState::new(&runtime);
    let app = app(state);
    tokio::spawn(heartbeat(runtime.clone()));
    tokio::spawn(reconcile_owned_connections(runtime));
    axum::serve(listener, app)
        .await
        .context("serving Codex traffic")
}

pub(crate) async fn reconcile_owned_connections(runtime: RouterRuntimeHandle) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        interval.tick().await;
        if let Err(error) = runtime.reconcile_owned_connections() {
            eprintln!("router worker failed to reconcile live connections: {error:#}");
        }
    }
}

fn path_and_query(uri: &Uri) -> &str {
    uri.path_and_query()
        .map_or(uri.path(), |value| value.as_str())
}

#[cfg(test)]
mod tests;
