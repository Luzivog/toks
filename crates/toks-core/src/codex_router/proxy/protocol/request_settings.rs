use serde_json::{Map, Value};

use crate::rotation::{ThreadOverride, ThreadRequestSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::codex_router::proxy) enum RequestEnvelope {
    HttpResponses,
    ResponseCreate,
}

pub(in crate::codex_router::proxy) struct RewrittenRequest {
    pub(in crate::codex_router::proxy) payload: String,
    pub(in crate::codex_router::proxy) automatic_tier_applied: bool,
}

pub(in crate::codex_router::proxy) fn requested_settings(payload: &str) -> ThreadRequestSettings {
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return ThreadRequestSettings::default();
    };
    ThreadRequestSettings {
        model: value
            .get("model")
            .and_then(Value::as_str)
            .filter(|model| !model.is_empty())
            .map(str::to_owned),
        reasoning_effort: value
            .get("reasoning")
            .and_then(Value::as_object)
            .and_then(|reasoning| reasoning.get("effort"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        service_tier: value
            .get("service_tier")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

pub(in crate::codex_router::proxy) fn rewrite_request(
    payload: &str,
    envelope: RequestEnvelope,
    request_override: Option<&ThreadOverride>,
    automatic_tier: Option<&str>,
) -> Option<RewrittenRequest> {
    let mut value: Value = serde_json::from_str(payload).ok()?;
    let object = value.as_object_mut()?;
    if !envelope.accepts(object.get("type")) {
        return None;
    }
    let mut changed = false;
    if let Some(model) = request_override.and_then(ThreadOverride::model) {
        changed |= set_string(object, "model", model);
    }
    if let Some(effort) = request_override.and_then(ThreadOverride::reasoning_effort) {
        changed |= set_reasoning_effort(object, effort);
    }
    let explicit_tier = request_override.and_then(ThreadOverride::service_tier);
    let automatic_tier_applied = match explicit_tier {
        Some(tier) => {
            changed |= set_string(object, "service_tier", tier);
            false
        }
        None => automatic_tier.is_some_and(|tier| {
            if requested_fast_tier(object) {
                false
            } else {
                let tier_changed = set_string(object, "service_tier", tier);
                changed |= tier_changed;
                tier_changed
            }
        }),
    };
    let payload = if changed {
        serde_json::to_string(&value).ok()?
    } else {
        payload.to_owned()
    };
    Some(RewrittenRequest {
        payload,
        automatic_tier_applied,
    })
}

impl RequestEnvelope {
    fn accepts(self, kind: Option<&Value>) -> bool {
        match self {
            Self::HttpResponses => {
                kind.is_none() || kind.and_then(Value::as_str) == Some("response.create")
            }
            Self::ResponseCreate => kind.and_then(Value::as_str) == Some("response.create"),
        }
    }
}

fn set_string(object: &mut Map<String, Value>, key: &str, value: &str) -> bool {
    if object.get(key).and_then(Value::as_str) == Some(value) {
        return false;
    }
    object.insert(key.to_owned(), Value::String(value.to_owned()));
    true
}

fn set_reasoning_effort(object: &mut Map<String, Value>, effort: &str) -> bool {
    let reasoning = object
        .entry("reasoning")
        .or_insert_with(|| Value::Object(Map::new()));
    if !reasoning.is_object() {
        *reasoning = Value::Object(Map::new());
    }
    set_string(
        reasoning.as_object_mut().expect("reasoning replaced above"),
        "effort",
        effort,
    )
}

fn requested_fast_tier(object: &Map<String, Value>) -> bool {
    object
        .get("service_tier")
        .and_then(Value::as_str)
        .is_some_and(|tier| matches!(tier, "fast" | "priority" | "ultrafast"))
}
