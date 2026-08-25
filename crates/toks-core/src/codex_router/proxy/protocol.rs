use serde_json::Value;

mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
mod request_frame;
mod request_settings;
#[cfg(test)]
mod request_settings_tests;
mod thread_identity;
mod usage_limit;

pub(super) use lifecycle::{ResponseLifecycle, ResponseLifecycleEnd};
pub(super) use request_frame::ClientRequestFrame;
pub(super) use request_settings::{
    requested_settings, rewrite_request, RequestEnvelope, RewrittenRequest,
};
pub(super) use thread_identity::ThreadIdentity;
pub(super) use usage_limit::{
    is_error_frame, stream_usage_block, usage_block, websocket_usage_block, UsageBlock,
};

pub(super) const RETRY_FRAME: &str = r#"{"type":"error","status":409,"error":{"type":"conflict_error","code":"toks_reconnect_required","message":"Toks needs a fresh connection to apply this task's current Codex route. Reconnecting to continue."}}"#;
pub(super) const ALL_UNAVAILABLE_FRAME: &str = r#"{"type":"error","status":429,"error":{"type":"usage_limit_reached","message":"All enrolled Codex subscriptions are unavailable."}}"#;
pub(super) const BAD_THREAD_FRAME: &str = r#"{"type":"error","status":400,"error":{"type":"invalid_request_error","message":"Conflicting Codex thread identity."}}"#;

/// Whether an upstream text frame establishes that this response has started.
/// Known session/control events do not; unknown text is conservative because
/// replaying after forwarding an unrecognized response frame could duplicate work.
pub(super) fn starts_response_delivery(payload: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return true;
    };
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return true;
    };
    kind.starts_with("response.") || is_error_frame(&value)
}

/// The model a `response.create` frame asks for. `model` sits at the top level
/// of the frame, alongside `instructions`, `service_tier` and `client_metadata`.
pub(super) fn requested_model(payload: &str) -> Option<String> {
    requested_settings(payload).model
}

pub(super) fn requested_service_tier(payload: &str) -> Option<String> {
    requested_settings(payload).service_tier
}

/// Rewrite a `response.create` frame to request `tier`, returning `None` when
/// the frame is not a `response.create`, is not a JSON object, or fails to
/// parse. A `None` return means "forward the frame untouched" — the turn still
/// runs, just at whatever tier the client asked for.
#[cfg(test)]
pub(super) fn with_service_tier(payload: &str, tier: &str) -> Option<String> {
    rewrite_request(payload, RequestEnvelope::ResponseCreate, None, Some(tier))
        .map(|rewritten| rewritten.payload)
}
