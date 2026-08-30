mod bridge;
mod connect;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{header, HeaderMap, Response, StatusCode, Uri};
use futures_util::StreamExt;

use super::headers::{activation_marker, response_headers, resume_marker};
use super::protocol::{ClientRequestFrame, ThreadIdentity, ALL_UNAVAILABLE_FRAME};
use super::{engine::RouteSelection, ProxyState};

pub(super) async fn forward(
    state: ProxyState,
    upgrade: WebSocketUpgrade,
    uri: Uri,
    headers: HeaderMap,
) -> Response<Body> {
    if activation_marker(&headers).is_present() {
        return empty(StatusCode::BAD_REQUEST);
    }
    let identity = ThreadIdentity::from_headers(&headers);
    if identity == ThreadIdentity::Denied {
        return empty(StatusCode::BAD_REQUEST);
    }
    let initial_thread = identity.into_thread();
    let marker = resume_marker(&headers);
    let resume_attempt = marker.attempt().map(str::to_owned);
    match connect::upstream(&state, &uri, &headers, initial_thread.as_ref(), marker).await {
        Ok(RouteSelection::Selected(connected)) => {
            connected_response(state, upgrade, connected, initial_thread, resume_attempt)
        }
        Ok(RouteSelection::Unavailable) => upgrade
            .on_upgrade(move |socket| unavailable(socket, state, initial_thread, true))
            .map(Body::new),
        Ok(RouteSelection::ResumeDenied) => {
            state.pause_after_resume_denial().await;
            upgrade
                .on_upgrade(move |socket| unavailable(socket, state, initial_thread, false))
                .map(Body::new)
        }
        Err(connect::ConnectFailure::Http) => empty(StatusCode::BAD_GATEWAY),
        Err(connect::ConnectFailure::Upstream) => empty(StatusCode::BAD_GATEWAY),
    }
}

fn connected_response(
    state: ProxyState,
    upgrade: WebSocketUpgrade,
    connected: connect::Connected,
    initial_thread: Option<crate::rotation::ThreadId>,
    resume_attempt: Option<String>,
) -> Response<Body> {
    let protocol = connect::selected_protocol(&connected.headers);
    let headers = response_headers(&connected.headers);
    let mut upgrade = upgrade;
    if let Some(protocol) = protocol.and_then(|value| value.to_str().ok().map(str::to_owned)) {
        upgrade = upgrade.protocols([protocol]);
    }
    let account = connected.credential.account_id;
    let upstream = connected.socket;
    let engine = state.engine.clone();
    let lifetime = state.lifetime.clone();
    let header_thread = initial_thread.clone();
    let mut response = upgrade
        .on_upgrade(move |client| async move {
            let _lifetime = lifetime;
            bridge::run(
                client,
                upstream,
                engine,
                account,
                header_thread,
                initial_thread,
                resume_attempt,
            )
            .await;
        })
        .map(Body::new);
    for (name, value) in &headers {
        if name != header::SEC_WEBSOCKET_ACCEPT && name != header::SEC_WEBSOCKET_PROTOCOL {
            response.headers_mut().append(name.clone(), value.clone());
        }
    }
    response
}

async fn unavailable(
    mut socket: WebSocket,
    state: ProxyState,
    header_thread: Option<crate::rotation::ThreadId>,
    queue: bool,
) {
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(text) => match ClientRequestFrame::from_payload(text.as_bytes()) {
                ClientRequestFrame::Denied => {
                    let _ = bridge::reject_thread(&mut socket).await;
                    return;
                }
                ClientRequestFrame::ResponseCreate(payload_identity) => {
                    let identity = header_thread
                        .clone()
                        .map_or(ThreadIdentity::Absent, ThreadIdentity::Unique)
                        .merge(payload_identity);
                    if identity == ThreadIdentity::Denied {
                        let _ = bridge::reject_thread(&mut socket).await;
                        return;
                    }
                    if queue {
                        if let Some(thread) = identity.into_thread() {
                            let _ = state.engine.waiting(&thread);
                        }
                    }
                    let _ = socket
                        .send(Message::Text(ALL_UNAVAILABLE_FRAME.into()))
                        .await;
                    return;
                }
                ClientRequestFrame::Other => {}
            },
            Message::Ping(payload) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Message::Binary(_) => {
                let _ = bridge::reject_thread(&mut socket).await;
                return;
            }
            Message::Close(_) => return,
            _ => {}
        }
    }
}

fn empty(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .expect("valid response")
}
