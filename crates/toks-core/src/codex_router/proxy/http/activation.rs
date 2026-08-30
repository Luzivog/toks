use axum::body::Body;
use axum::http::{Response, StatusCode};

use crate::codex_router::proxy::headers::{activation_marker, resume_marker, ActivationMarker};
use crate::rotation::ThreadId;

use super::attempt::Attempt;
use super::request_body::CodexHttpBody;
use super::response::{plain, usage_unavailable};
use super::{send, ProxyState};

pub(super) async fn forward(
    state: &ProxyState,
    parts: &axum::http::request::Parts,
    body: &CodexHttpBody,
    thread: Option<&ThreadId>,
) -> Option<Response<Body>> {
    let marker = activation_marker(&parts.headers);
    if marker == ActivationMarker::Absent {
        return None;
    }
    let Some(thread) = thread.filter(|_| !resume_marker(&parts.headers).is_present()) else {
        return Some(usage_unavailable());
    };
    let credential = match state
        .engine
        .select_for_activation_thread(thread, marker)
        .await
    {
        Ok(Some(credential)) => credential,
        Ok(None) => return Some(usage_unavailable()),
        Err(_) => {
            return Some(plain(
                StatusCode::BAD_GATEWAY,
                "Codex credential is unavailable",
            ))
        }
    };
    let attempt = marker.attempt().expect("selected activation marker");
    if state
        .engine
        .observe_activation_route(attempt, thread, &credential.account_id)
        .is_err()
    {
        let _ = state
            .engine
            .release_reservation(&credential.account_id, thread);
        return Some(usage_unavailable());
    }
    Some(
        match send(state, parts, body, credential, &Some(thread.clone()), None).await {
            Attempt::Response(response) => response,
            Attempt::TryNext(_) | Attempt::RetrySameAccountAtStandardTier => usage_unavailable(),
            Attempt::Failed => plain(StatusCode::BAD_GATEWAY, "OpenAI is unavailable"),
        },
    )
}
