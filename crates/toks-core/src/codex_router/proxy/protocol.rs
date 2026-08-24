use serde_json::Value;

mod lifecycle;
mod request_frame;
mod thread_identity;
mod usage_limit;

pub(super) use lifecycle::{ResponseLifecycle, ResponseLifecycleEnd};
pub(super) use request_frame::ClientRequestFrame;
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
    let value: Value = serde_json::from_str(payload).ok()?;
    value
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
}

pub(super) fn requested_service_tier(payload: &str) -> Option<String> {
    let value: Value = serde_json::from_str(payload).ok()?;
    value
        .get("service_tier")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Rewrite a `response.create` frame to request `tier`, returning `None` when
/// the frame is not a `response.create`, is not a JSON object, or fails to
/// parse. A `None` return means "forward the frame untouched" — the turn still
/// runs, just at whatever tier the client asked for.
pub(super) fn with_service_tier(payload: &str, tier: &str) -> Option<String> {
    let mut value: Value = serde_json::from_str(payload).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("response.create") {
        return None;
    }
    let object = value.as_object_mut()?;
    if object
        .get("service_tier")
        .and_then(Value::as_str)
        .is_some_and(|tier| matches!(tier, "fast" | "priority" | "ultrafast"))
    {
        return Some(payload.to_owned());
    }
    object.insert("service_tier".into(), Value::String(tier.to_owned()));
    serde_json::to_string(&value).ok()
}
