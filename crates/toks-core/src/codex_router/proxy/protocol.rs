use axum::http::HeaderMap;
use serde_json::Value;

use crate::rotation::{ThreadId, UnixMillis};

pub(super) const RETRY_FRAME: &str = r#"{"type":"error","status":400,"error":{"type":"invalid_request_error","code":"websocket_connection_limit_reached","message":"Responses websocket connection limit reached (60 minutes). Create a new websocket connection to continue."}}"#;
pub(super) const ALL_UNAVAILABLE_FRAME: &str = r#"{"type":"error","status":429,"error":{"type":"usage_limit_reached","message":"All enrolled Codex subscriptions are unavailable."}}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UsageBlock {
    pub resets_at: Option<UnixMillis>,
}

pub(super) fn usage_block(status: u16, payload: &[u8]) -> Option<UsageBlock> {
    if status != 429 {
        return None;
    }
    let value: Value = serde_json::from_slice(payload).ok()?;
    (value.pointer("/error/type").and_then(Value::as_str) == Some("usage_limit_reached")).then(
        || UsageBlock {
            resets_at: value.pointer("/error/resets_at").and_then(epoch_millis),
        },
    )
}

pub(super) fn websocket_usage_block(payload: &str) -> Option<UsageBlock> {
    let value: Value = serde_json::from_str(payload).ok()?;
    let status = value.get("status")?.as_u64()?.try_into().ok()?;
    usage_block(status, payload.as_bytes())
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
