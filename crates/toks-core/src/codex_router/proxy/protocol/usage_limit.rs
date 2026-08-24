use serde_json::Value;

use crate::rotation::{
    ThreadId, UnixMillis, UsageLimitClassification, UsageLimitEvidence, UsageLimitIncident,
    UsageLimitPhase, UsageLimitTier,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codex_router::proxy) struct UsageBlock {
    pub resets_at: Option<UnixMillis>,
    evidence: UsageLimitEvidence,
}

impl UsageBlock {
    pub(in crate::codex_router::proxy) fn incident(
        &self,
        thread_id: Option<ThreadId>,
        model: Option<&str>,
        tier: UsageLimitTier,
        phase: UsageLimitPhase,
    ) -> UsageLimitIncident {
        UsageLimitIncident::new(thread_id, model, tier, phase, self.evidence.clone())
    }
}

const USAGE_LIMIT_MARKERS: [&str; 2] = ["usage limit", "hit your usage"];

pub(in crate::codex_router::proxy) fn usage_block(
    status: u16,
    payload: &[u8],
) -> Option<UsageBlock> {
    if status != 429 {
        return None;
    }
    classify(payload, Some(status))
}

pub(in crate::codex_router::proxy) fn websocket_usage_block(payload: &str) -> Option<UsageBlock> {
    stream_usage_block(payload.as_bytes())
}

pub(in crate::codex_router::proxy) fn stream_usage_block(payload: &[u8]) -> Option<UsageBlock> {
    classify(payload, None)
}

fn classify(payload: &[u8], status: Option<u16>) -> Option<UsageBlock> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    let structured =
        value.pointer("/error/type").and_then(Value::as_str) == Some("usage_limit_reached");
    let message_based =
        is_error_frame(&value) && message_text(&value).is_some_and(has_usage_marker);
    let classification = if structured {
        UsageLimitClassification::StructuredError
    } else if message_based {
        UsageLimitClassification::ErrorMessage
    } else {
        return None;
    };
    Some(UsageBlock {
        resets_at: usage_reset(&value),
        evidence: UsageLimitEvidence::from_upstream(
            classification,
            status.or_else(|| json_status(&value)),
            value.get("type").and_then(Value::as_str),
            value.pointer("/error/type").and_then(Value::as_str),
            value.pointer("/error/code").and_then(Value::as_str),
            payload,
        ),
    })
}

fn json_status(value: &Value) -> Option<u16> {
    value
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| status.try_into().ok())
}

pub(in crate::codex_router::proxy) fn is_error_frame(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("error" | "turn.failed" | "response.failed" | "stream.error" | "stream_error")
    ) || value.get("error").is_some()
        || value.get("status").and_then(Value::as_u64) == Some(429)
}

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

fn epoch_millis(value: &Value) -> Option<UnixMillis> {
    let value = value.as_i64().or_else(|| value.as_str()?.parse().ok())?;
    let millis = if value < 10_000_000_000 {
        value.checked_mul(1_000)?
    } else {
        value
    };
    Some(UnixMillis::new(millis))
}
