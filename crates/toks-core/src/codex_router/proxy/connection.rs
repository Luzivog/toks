use anyhow::Result;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnectionBuilder;
use hyper_util::service::TowerToHyperService;
use std::sync::Arc;

use super::{app, Engine, InboundTokens, ProxyState, RouterRuntimeHandle, Upstream};

/// Worker-scoped proxy dependencies shared by every accepted connection.
///
/// A worker creates this once. Each served socket receives its own lifetime
/// guard while admissions and the upstream connection pool remain shared.
#[derive(Clone)]
pub(crate) struct ConnectionService {
    engine: Arc<Engine>,
    tokens: Arc<InboundTokens>,
    http: reqwest::Client,
    upstream: Upstream,
}

impl ConnectionService {
    pub(crate) fn new(runtime: &RouterRuntimeHandle) -> Self {
        Self {
            engine: runtime.engine.clone(),
            tokens: Arc::new(InboundTokens::new(runtime.credentials.clone())),
            http: reqwest::Client::new(),
            upstream: Upstream::chatgpt(),
        }
    }

    #[cfg(test)]
    pub(super) fn from_state(state: ProxyState) -> Self {
        Self {
            engine: state.engine,
            tokens: state.tokens,
            http: state.http,
            upstream: state.upstream,
        }
    }

    pub(crate) async fn serve(
        &self,
        stream: tokio::net::TcpStream,
        lifetime: ConnectionLifetime,
    ) -> Result<()> {
        serve_state_connection(self.state(lifetime), stream).await
    }

    pub(super) fn state(&self, lifetime: ConnectionLifetime) -> ProxyState {
        ProxyState {
            engine: self.engine.clone(),
            tokens: self.tokens.clone(),
            http: self.http.clone(),
            upstream: self.upstream.clone(),
            lifetime,
            #[cfg(test)]
            resume_denial_gate: None,
        }
    }
}

pub(crate) struct ConnectionLifetime(Arc<LifetimeInner>);

impl Clone for ConnectionLifetime {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

struct LifetimeInner(Option<Box<dyn FnOnce() + Send + Sync>>);

impl ConnectionLifetime {
    pub(crate) fn new(on_close: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self(Arc::new(LifetimeInner(Some(Box::new(on_close)))))
    }

    #[cfg(test)]
    fn noop() -> Self {
        Self::new(|| {})
    }
}

impl Drop for LifetimeInner {
    fn drop(&mut self) {
        if let Some(on_close) = self.0.take() {
            on_close();
        }
    }
}

/// Serves one already-accepted client socket until its HTTP connection closes.
///
/// The connection task has no coordinator dependency after handoff, so a
/// coordinator restart cannot interrupt its response stream.
#[cfg(test)]
pub(crate) async fn serve_connection(
    runtime: RouterRuntimeHandle,
    stream: tokio::net::TcpStream,
) -> Result<()> {
    ConnectionService::new(&runtime)
        .serve(stream, ConnectionLifetime::noop())
        .await
}

pub(super) async fn serve_state_connection(
    state: ProxyState,
    stream: tokio::net::TcpStream,
) -> Result<()> {
    let service = TowerToHyperService::new(app(state));
    ConnectionBuilder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(TokioIo::new(stream), service)
        .await
        .map_err(|error| anyhow::anyhow!("serving handed-off Codex connection: {error}"))
}
