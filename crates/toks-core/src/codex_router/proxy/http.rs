use std::collections::BTreeSet;

use axum::body::{to_bytes, Body};
use axum::http::{Request, Response, StatusCode};

use super::headers::upstream_headers;
use super::lease::StreamLease;
use super::protocol::thread_id;
use super::types::RouteCredential;
use super::ProxyState;
use attempt::{classify_response, request_body, Attempt};

mod attempt;
mod stream;

const MAX_REQUEST_BYTES: usize = 128 * 1024 * 1024;

pub(super) async fn forward(state: ProxyState, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, MAX_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(_) => return plain(StatusCode::PAYLOAD_TOO_LARGE, "Codex request is too large"),
    };
    let thread =
        super::protocol::thread_id_from_headers(&parts.headers).or_else(|| thread_id(&body));
    let mut skipped = BTreeSet::new();
    loop {
        let credential = match state
            .engine
            .select_for_thread(thread.as_ref(), &skipped)
            .await
        {
            Ok(Some(credential)) => credential,
            Ok(None) => {
                if let Some(thread) = &thread {
                    let _ = state.engine.waiting(thread);
                }
                return usage_unavailable();
            }
            Err(_) => return plain(StatusCode::BAD_GATEWAY, "Codex credential is unavailable"),
        };
        match send(&state, &parts, body.clone(), credential, &thread).await {
            Attempt::Response(response) => return response,
            Attempt::TryNext(account) => {
                skipped.insert(account);
            }
            Attempt::RetrySameAccountAtStandardTier => {}
            Attempt::Failed => return plain(StatusCode::BAD_GATEWAY, "OpenAI is unavailable"),
        }
    }
}

async fn send(
    state: &ProxyState,
    parts: &axum::http::request::Parts,
    body: axum::body::Bytes,
    mut credential: RouteCredential,
    thread: &Option<crate::rotation::ThreadId>,
) -> Attempt {
    let mut refreshed = false;
    loop {
        let lease = match thread {
            Some(thread) => {
                match StreamLease::open(state.engine.clone(), &credential.account_id, thread) {
                    Ok(Some(lease)) => Some(lease),
                    Ok(None) => return Attempt::TryNext(credential.account_id),
                    Err(_) => return Attempt::Failed,
                }
            }
            None => None,
        };
        let (attempt_body, forced_fast) = request_body(
            state,
            lease
                .as_ref()
                .map_or(super::engine::RouteTier::Original, StreamLease::tier),
            thread,
            body.clone(),
        );
        let request = state
            .http
            .request(parts.method.clone(), state.upstream.http_url(&parts.uri))
            .headers(upstream_headers(&parts.headers, &credential, false))
            .body(attempt_body);
        let response = match request.send().await {
            Ok(response) => response,
            Err(_) => return Attempt::Failed,
        };
        if response.status() == StatusCode::UNAUTHORIZED {
            if refreshed {
                drop(lease);
                let _ = state.engine.permanent_auth_failure(&credential.account_id);
                return Attempt::TryNext(credential.account_id);
            }
            if let Some(thread) = thread {
                if state
                    .engine
                    .reserve_retry(&credential.account_id, thread)
                    .is_err()
                {
                    return Attempt::Failed;
                }
            }
            drop(lease);
            refreshed = true;
            match state.engine.refresh(&credential.account_id).await {
                Ok(Some(updated)) => credential = updated,
                Ok(None) => {
                    release_retry(state, thread, &credential.account_id);
                    return Attempt::TryNext(credential.account_id);
                }
                Err(_) => {
                    release_retry(state, thread, &credential.account_id);
                    return Attempt::Failed;
                }
            }
            continue;
        }
        return classify_response(
            state,
            response,
            credential.account_id,
            thread,
            lease,
            forced_fast,
        )
        .await;
    }
}

fn release_retry(
    state: &ProxyState,
    thread: &Option<crate::rotation::ThreadId>,
    account: &crate::accounts::AccountId,
) {
    if let Some(thread) = thread {
        let _ = state.engine.release_reservation(account, thread);
    }
}

fn build_response(
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn plain(status: StatusCode, message: &'static str) -> Response<Body> {
    build_response(status, Default::default(), Body::from(message))
}

fn usage_unavailable() -> Response<Body> {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    build_response(
        StatusCode::TOO_MANY_REQUESTS,
        headers,
        Body::from(super::protocol::ALL_UNAVAILABLE_FRAME),
    )
}
