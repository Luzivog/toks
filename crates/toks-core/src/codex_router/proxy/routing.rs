use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State, WebSocketUpgrade};
use axum::http::{header, Response, StatusCode};
use axum::routing::get;
use axum::Router;

use super::headers::bearer_token;
use super::{http, websocket, ProxyState, CODEX_PATH, HEALTH_BODY};

pub(super) fn app(state: ProxyState) -> Router {
    Router::new()
        .route("/health", get(|| async { HEALTH_BODY }))
        .fallback(dispatch)
        .with_state(state)
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

fn empty(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .expect("valid response")
}
