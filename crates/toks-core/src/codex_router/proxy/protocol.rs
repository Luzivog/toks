use axum::http::HeaderMap;
use serde_json::Value;

use crate::rotation::{ThreadId, UnixMillis};

pub(super) const RETRY_FRAME: &str = r#"{"type":"error","status":400,"error":{"type":"invalid_request_error","code":"websocket_connection_limit_reached","message":"Responses websocket connection limit reached (60 minutes). Create a new websocket connection to continue."}}"#;
pub(super) const ALL_UNAVAILABLE_FRAME: &str = r#"{"type":"error","status":429,"error":{"type":"usage_limit_reached","message":"All enrolled Codex subscriptions are unavailable."}}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UsageBlock {
    pub resets_at: Option<UnixMillis>,
}

/// Substrings that identify an upstream usage-limit message. Real Codex frames
/// phrase it as "You've hit your usage limit. …"; matched case-insensitively.
const USAGE_LIMIT_MARKERS: [&str; 2] = ["usage limit", "hit your usage"];

pub(super) fn usage_block(status: u16, payload: &[u8]) -> Option<UsageBlock> {
    if status != 429 {
        return None;
    }
    let value: Value = serde_json::from_slice(payload).ok()?;
    usage_limit(&value)
}

pub(super) fn websocket_usage_block(payload: &str) -> Option<UsageBlock> {
    // Real upstream usage-limit frames carry no top-level `status`, so the
    // stream detector keys off the frame shape and message instead.
    let value: Value = serde_json::from_str(payload).ok()?;
    usage_limit(&value)
}

/// Tolerant usage-limit detector shared by the HTTP and WebSocket paths. Matches
/// either the structured `usage_limit_reached` shape (legacy and the router's
/// own synthetic frames) or a real upstream error/failure frame whose message
/// names a usage limit.
fn usage_limit(value: &Value) -> Option<UsageBlock> {
    let structured =
        value.pointer("/error/type").and_then(Value::as_str) == Some("usage_limit_reached");
    let message_based = is_error_frame(value) && message_text(value).is_some_and(has_usage_marker);
    (structured || message_based).then(|| UsageBlock {
        resets_at: usage_reset(value),
    })
}

/// True when the frame reports an error/failure rather than normal model output.
/// Gating on this keeps a legitimate "usage limit" mention inside streamed model
/// text from being misread as a block.
fn is_error_frame(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("error" | "turn.failed" | "response.failed" | "stream.error" | "stream_error")
    ) || value.get("error").is_some()
        || value.get("status").and_then(Value::as_u64) == Some(429)
}

/// The human-readable error text, whether carried top-level or under `error`.
fn message_text(value: &Value) -> Option<&str> {
    value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/error/message").and_then(Value::as_str))
}

fn has_usage_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    USAGE_LIMIT_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// A structured reset timestamp when the frame carries one. Real frames embed
/// the reset only in prose ("try again at …"); we deliberately skip parsing that
/// and let `Engine::block` fall back to the limits snapshot's reset instead.
fn usage_reset(value: &Value) -> Option<UnixMillis> {
    [
        "/error/resets_at",
        "/resets_at",
        "/error/reset_at",
        "/reset_at",
    ]
    .into_iter()
    .find_map(|pointer| value.pointer(pointer).and_then(epoch_millis))
}

pub(super) fn thread_id(payload: &[u8]) -> Option<ThreadId> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    value
        .pointer("/client_metadata/thread_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(ThreadId::new)
}

pub(super) fn thread_id_from_headers(headers: &HeaderMap) -> Option<ThreadId> {
    ["thread-id", "x-thread-id", "session-id", "x-session-id"]
        .into_iter()
        .find_map(|name| headers.get(name)?.to_str().ok())
        .filter(|id| !id.is_empty())
        .map(ThreadId::new)
}

pub(super) fn is_response_create(payload: &str) -> bool {
    serde_json::from_str::<Value>(payload)
        .ok()
        .is_some_and(|value| value.get("type").and_then(Value::as_str) == Some("response.create"))
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

pub(super) fn response_terminal(payload: &str) -> bool {
    matches!(
        event_type(payload).as_deref(),
        Some("response.completed" | "response.failed" | "response.incomplete")
    )
}

pub(super) fn model_visible_output(payload: &str) -> bool {
    let Some(kind) = event_type(payload) else {
        return false;
    };
    kind.starts_with("response.")
        && !matches!(
            kind.as_str(),
            "response.created" | "response.in_progress" | "response.queued"
        )
        && kind != "response.completed"
}

fn event_type(payload: &str) -> Option<String> {
    let value: Value = serde_json::from_str(payload).ok()?;
    value.get("type")?.as_str().map(str::to_owned)
}

fn epoch_millis(value: &Value) -> Option<UnixMillis> {
    let value = value.as_i64().or_else(|| value.as_str()?.parse().ok())?;
    let millis = if value < 10_000_000_000 {
        value.checked_mul(1_000)?
    } else {
        value
    };
    Some(UnixMillis::new(millis))
}
