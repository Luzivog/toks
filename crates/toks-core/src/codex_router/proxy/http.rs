use std::collections::BTreeSet;

use axum::body::{to_bytes, Body};
use axum::http::{Request, Response, StatusCode};

use super::engine::RouteSelection;
use super::headers::{resume_marker, upstream_headers};
use super::lease::StreamLease;
use super::protocol::ThreadIdentity;
use super::types::RouteCredential;
use super::ProxyState;
use attempt::{classify_response, Attempt, ResponseContext};
use request_body::CodexHttpBody;
use response::{plain, usage_unavailable};

mod attempt;
mod prepare;
#[cfg(test)]
mod prepare_tests;
mod request_body;
mod response;
mod stream;

const MAX_REQUEST_BYTES: usize = 128 * 1024 * 1024;

pub(super) async fn forward(state: ProxyState, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let wire_body = match to_bytes(body, MAX_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(_) => return plain(StatusCode::PAYLOAD_TOO_LARGE, "Codex request is too large"),
    };
    let body = match CodexHttpBody::decode(&parts.headers, wire_body, MAX_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(error) => return plain(error.status(), error.message()),
    };
    let identity = ThreadIdentity::from_headers(&parts.headers)
        .merge(ThreadIdentity::from_payload(body.decoded()));
    if identity == ThreadIdentity::Denied {
        return plain(StatusCode::BAD_REQUEST, "Conflicting Codex thread identity");
    }
    let thread = identity.into_thread();
    let marker = resume_marker(&parts.headers);
    let resume_attempt = marker.attempt().map(str::to_owned);
    if thread.is_none() && marker.is_present() {
        state.pause_after_resume_denial().await;
        return usage_unavailable();
    }
    let mut skipped = BTreeSet::new();
    loop {
        let credential = match state
            .engine
            .select_for_thread_authorized(thread.as_ref(), marker, &skipped)
            .await
        {
            Ok(RouteSelection::Selected(credential)) => credential,
            Ok(RouteSelection::Unavailable) => {
                if let Some(thread) = &thread {
                    let _ = state.engine.waiting(thread);
                }
                return usage_unavailable();
            }
            Ok(RouteSelection::ResumeDenied) => {
                state.pause_after_resume_denial().await;
                return usage_unavailable();
            }
            Err(_) => return plain(StatusCode::BAD_GATEWAY, "Codex credential is unavailable"),
        };
        match send(
            &state,
            &parts,
            &body,
            credential,
            &thread,
            resume_attempt.as_deref(),
        )
        .await
        {
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
    body: &CodexHttpBody,
    mut credential: RouteCredential,
    thread: &Option<crate::rotation::ThreadId>,
    resume_attempt: Option<&str>,
) -> Attempt {
    let mut refreshed = false;
    loop {
        let lease = match thread {
            Some(thread) => {
                match StreamLease::open(
                    state.engine.clone(),
                    &credential.account_id,
                    thread,
                    resume_attempt,
                ) {
                    Ok(Some(lease)) => Some(lease),
                    Ok(None) => return Attempt::TryNext(credential.account_id),
                    Err(_) => return Attempt::Failed,
                }
            }
            None => None,
        };
        let Ok(prepared) = prepare::request_body(
            state,
            lease
                .as_ref()
                .map_or(super::engine::RouteTier::Original, StreamLease::tier),
            thread,
            body,
            parts.uri.path().strip_prefix(super::CODEX_PATH) == Some("/responses"),
            MAX_REQUEST_BYTES,
        )
        .await
        else {
            return Attempt::Failed;
        };
        let request = state
            .http
            .request(parts.method.clone(), state.upstream.http_url(&parts.uri))
            .headers(upstream_headers(&parts.headers, &credential, false))
            .body(prepared.wire);
        let response = match request.send().await {
            Ok(response) => response,
            Err(_) => return Attempt::Failed,
        };
        if response.status() == StatusCode::UNAUTHORIZED {
            if refreshed {
                drop(lease);
                let _ = state.engine.permanent_auth_failure(&credential);
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
            match state.engine.refresh(&credential).await {
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
            ResponseContext {
                account: credential.account_id,
                thread: thread.clone(),
                lease,
                forced_fast: prepared.forced_fast,
                model: prepared.model,
                request_tier: prepared.tier,
            },
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
