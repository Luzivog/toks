use std::collections::BTreeSet;

use axum::body::{to_bytes, Body};
use axum::http::{Request, Response, StatusCode};
use futures_util::{stream, StreamExt};

use crate::accounts::AccountId;

use super::headers::{response_headers, upstream_headers};
use super::lease::StreamLease;
use super::protocol::{thread_id, usage_block};
use super::types::RouteCredential;
use super::ProxyState;

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
        let credential = match state.engine.select(&skipped).await {
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
        let lease = thread.as_ref().and_then(|thread| {
            StreamLease::open(state.engine.clone(), &credential.account_id, thread).ok()
        });
        let request = state
            .http
            .request(parts.method.clone(), state.upstream.http_url(&parts.uri))
            .headers(upstream_headers(&parts.headers, &credential, false))
            .body(body.clone());
        let response = match request.send().await {
            Ok(response) => response,
            Err(_) => return Attempt::Failed,
        };
        if response.status() == StatusCode::UNAUTHORIZED {
            drop(lease);
            if refreshed {
                let _ = state.engine.permanent_auth_failure(&credential.account_id);
                return Attempt::TryNext(credential.account_id);
            }
            refreshed = true;
            match state.engine.refresh(&credential.account_id).await {
                Ok(Some(updated)) => credential = updated,
                Ok(None) => return Attempt::TryNext(credential.account_id),
                Err(_) => return Attempt::Failed,
            }
            continue;
        }
        return classify_response(state, response, credential.account_id, thread, lease).await;
    }
}

async fn classify_response(
    state: &ProxyState,
    response: reqwest::Response,
    account: AccountId,
    thread: &Option<crate::rotation::ThreadId>,
    lease: Option<StreamLease>,
) -> Attempt {
    let status = response.status();
    let headers = response_headers(response.headers());
    if status == StatusCode::TOO_MANY_REQUESTS {
        let body = response.bytes().await.unwrap_or_default();
        if let Some(block) = usage_block(status.as_u16(), &body) {
            let _ = state.engine.block(&account, block.resets_at);
            drop(lease);
            if state.engine.eligible_account().ok().flatten().is_some() {
                return Attempt::TryNext(account);
            }
            if let Some(thread) = thread {
                let _ = state.engine.waiting(thread);
            }
        }
        return Attempt::Response(build_response(status, headers, Body::from(body)));
    }
    let stream = stream::unfold(
        (response.bytes_stream(), lease),
        |(mut body, lease)| async move { body.next().await.map(|chunk| (chunk, (body, lease))) },
    );
    Attempt::Response(build_response(status, headers, Body::from_stream(stream)))
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

enum Attempt {
    Response(Response<Body>),
    TryNext(AccountId),
    Failed,
}
