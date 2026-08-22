mod bridge;
mod connect;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{header, HeaderMap, Response, StatusCode, Uri};
use futures_util::StreamExt;

use super::headers::response_headers;
use super::protocol::{
    is_response_create, thread_id, thread_id_from_headers, ALL_UNAVAILABLE_FRAME,
};
use super::ProxyState;

pub(super) async fn forward(
    state: ProxyState,
    upgrade: WebSocketUpgrade,
    uri: Uri,
    headers: HeaderMap,
) -> Response<Body> {
    let initial_thread = thread_id_from_headers(&headers);
    match connect::upstream(&state, &uri, &headers).await {
        Ok(Some(connected)) => {
            let protocol = connect::selected_protocol(&connected.headers);
            let account = connected.credential.account_id;
            let upstream = connected.socket;
            let engine = state.engine.clone();
            let initial_thread = initial_thread.clone();
            let mut upgrade = upgrade;
            if let Some(protocol) =
                protocol.and_then(|value| value.to_str().ok().map(str::to_owned))
            {
                upgrade = upgrade.protocols([protocol]);
            }
            let mut response = upgrade
                .on_upgrade(move |client| {
                    bridge::run(client, upstream, engine, account, initial_thread)
                })
                .map(Body::new);
            let headers = response_headers(&connected.headers);
            for (name, value) in &headers {
                if name != header::SEC_WEBSOCKET_ACCEPT && name != header::SEC_WEBSOCKET_PROTOCOL {
                    response.headers_mut().append(name.clone(), value.clone());
                }
            }
            response
        }
        Ok(None) => upgrade
            .on_upgrade(move |socket| unavailable(socket, state, initial_thread))
            .map(Body::new),
        Err(connect::ConnectFailure::Http(status)) => empty(status),
        Err(connect::ConnectFailure::Upstream) => empty(StatusCode::BAD_GATEWAY),
    }
}

async fn unavailable(
    mut socket: WebSocket,
    state: ProxyState,
    initial_thread: Option<crate::rotation::ThreadId>,
) {
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(text) if is_response_create(&text) => {
                if let Some(thread) = thread_id(text.as_bytes()).or_else(|| initial_thread.clone())
                {
                    let _ = state.engine.waiting(&thread);
                }
                let _ = socket
                    .send(Message::Text(ALL_UNAVAILABLE_FRAME.into()))
                    .await;
                return;
            }
            Message::Ping(payload) => {
                let _ = socket.send(Message::Pong(payload)).await;
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
