use std::collections::BTreeSet;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use super::super::headers::upstream_headers;
use super::super::protocol::usage_block;
use super::super::types::RouteCredential;
use super::super::ProxyState;

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
) -> Result<Option<Connected>, ConnectFailure> {
    let mut skipped = BTreeSet::new();
    loop {
        let Some(mut credential) = state
            .engine
            .select(&skipped)
            .await
            .map_err(|_| ConnectFailure::Upstream)?
        else {
            return Ok(None);
        };
        let mut refreshed = false;
        loop {
            match connect(state, uri, incoming, &credential).await {
                Ok(connected) => return Ok(Some(connected)),
                Err(WsError::Http(response)) if response.status() == StatusCode::UNAUTHORIZED => {
                    if refreshed {
                        let _ = state.engine.permanent_auth_failure(&credential.account_id);
                        skipped.insert(credential.account_id);
                        break;
                    }
                    refreshed = true;
                    match state.engine.refresh(&credential.account_id).await {
                        Ok(Some(updated)) => credential = updated,
                        Ok(None) => {
                            skipped.insert(credential.account_id);
                            break;
                        }
                        Err(_) => return Err(ConnectFailure::Upstream),
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
                    let block = response
                        .body()
                        .as_deref()
                        .and_then(|body| usage_block(response.status().as_u16(), body))
                        .expect("guard classified usage block");
                    let _ = state.engine.block(&credential.account_id, block.resets_at);
                    skipped.insert(credential.account_id);
                    break;
                }
                Err(WsError::Http(response)) => {
                    return Err(ConnectFailure::Http(response.status()))
                }
                Err(_) => return Err(ConnectFailure::Upstream),
            }
        }
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
    Http(StatusCode),
    Upstream,
}

pub(super) fn selected_protocol(headers: &HeaderMap) -> Option<HeaderValue> {
    headers.get(header::SEC_WEBSOCKET_PROTOCOL).cloned()
}
