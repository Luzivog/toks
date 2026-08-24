use std::collections::BTreeSet;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::codex_router::proxy::headers::upstream_headers;
use crate::codex_router::proxy::protocol::usage_block;
use crate::codex_router::proxy::types::RouteCredential;
use crate::codex_router::proxy::ProxyState;
use crate::codex_router::proxy::{engine::RouteSelection, headers::ResumeMarker};
use crate::rotation::{ThreadId, UsageLimitPhase, UsageLimitTier};

pub(super) type UpstreamSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(super) struct Connected {
    pub credential: RouteCredential,
    pub socket: UpstreamSocket,
    pub headers: HeaderMap,
}

pub(super) async fn upstream(
    state: &ProxyState,
    uri: &Uri,
    incoming: &HeaderMap,
    thread: Option<&ThreadId>,
    marker: ResumeMarker<'_>,
) -> Result<RouteSelection<Connected>, ConnectFailure> {
    let mut skipped = BTreeSet::new();
    loop {
        let mut credential = match state
            .engine
            .select_for_thread_authorized(thread, marker, &skipped)
            .await
            .map_err(|_| ConnectFailure::Upstream)?
        {
            RouteSelection::Selected(credential) => credential,
            RouteSelection::ResumeDenied => return Ok(RouteSelection::ResumeDenied),
            RouteSelection::Unavailable => return Ok(RouteSelection::Unavailable),
        };
        let mut refreshed = false;
        loop {
            match connect(state, uri, incoming, &credential).await {
                Ok(connected) => return Ok(RouteSelection::Selected(connected)),
                Err(WsError::Http(response)) if response.status() == StatusCode::UNAUTHORIZED => {
                    if refreshed {
                        release(state, thread, &credential.account_id);
                        let _ = state.engine.permanent_auth_failure(&credential);
                        skipped.insert(credential.account_id);
                        break;
                    }
                    refreshed = true;
                    match state.engine.refresh(&credential).await {
                        Ok(Some(updated)) => credential = updated,
                        Ok(None) => {
                            release(state, thread, &credential.account_id);
                            skipped.insert(credential.account_id);
                            break;
                        }
                        Err(_) => {
                            release(state, thread, &credential.account_id);
                            return Err(ConnectFailure::Upstream);
                        }
                    }
                }
                Err(WsError::Http(response))
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        && response
                            .body()
                            .as_deref()
                            .and_then(|body| usage_block(response.status().as_u16(), body))
                            .is_some() =>
                {
                    release(state, thread, &credential.account_id);
                    let block = response
                        .body()
                        .as_deref()
                        .and_then(|body| usage_block(response.status().as_u16(), body))
                        .expect("guard classified usage block");
                    state
                        .engine
                        .upstream_admission_usage_limited(
                            &credential.account_id,
                            block.resets_at,
                            block.incident(
                                thread.cloned(),
                                None,
                                UsageLimitTier::unspecified(),
                                UsageLimitPhase::WebSocketHandshake,
                            ),
                        )
                        .map_err(|_| ConnectFailure::Upstream)?;
                    skipped.insert(credential.account_id);
                    break;
                }
                Err(WsError::Http(_)) => {
                    release(state, thread, &credential.account_id);
                    return Err(ConnectFailure::Http);
                }
                Err(_) => {
                    release(state, thread, &credential.account_id);
                    return Err(ConnectFailure::Upstream);
                }
            }
        }
    }
}

fn release(state: &ProxyState, thread: Option<&ThreadId>, account: &crate::accounts::AccountId) {
    if let Some(thread) = thread {
        let _ = state.engine.release_reservation(account, thread);
    }
}

async fn connect(
    state: &ProxyState,
    uri: &Uri,
    incoming: &HeaderMap,
    credential: &RouteCredential,
) -> Result<Connected, WsError> {
    let url = state.upstream.ws_url(uri);
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .extend(upstream_headers(incoming, credential, true));
    if let Some(protocol) = incoming.get(header::SEC_WEBSOCKET_PROTOCOL) {
        request
            .headers_mut()
            .insert(header::SEC_WEBSOCKET_PROTOCOL, protocol.clone());
    }
    let (socket, response) = connect_async(request).await?;
    Ok(Connected {
        credential: credential.clone(),
        socket,
        headers: response.headers().clone(),
    })
}

#[derive(Debug)]
pub(super) enum ConnectFailure {
    Http,
    Upstream,
}

pub(super) fn selected_protocol(headers: &HeaderMap) -> Option<HeaderValue> {
    headers.get(header::SEC_WEBSOCKET_PROTOCOL).cloned()
}
