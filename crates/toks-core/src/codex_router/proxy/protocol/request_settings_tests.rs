use serde_json::json;

use super::{requested_settings, rewrite_request, RequestEnvelope};
use crate::rotation::{RotationSettings, ThreadId, ThreadOverrideChange};

#[test]
fn request_settings_parse_nested_reasoning() {
    let settings = requested_settings(
        &json!({
            "type":"response.create",
            "model":"gpt-5.6-sol",
            "reasoning":{"effort":"xhigh"},
            "service_tier":"default"
        })
        .to_string(),
    );

    assert_eq!(settings.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(settings.reasoning_effort.as_deref(), Some("xhigh"));
    assert_eq!(settings.service_tier.as_deref(), Some("default"));
    assert_eq!(requested_settings("not-json"), Default::default());
}

#[test]
fn request_override_preserves_reasoning_keys_and_downgrades_tier() {
    let thread = ThreadId::new("thread");
    let mut settings = RotationSettings::default();
    settings
        .set_thread_override(&thread, ThreadOverrideChange::Model(Some("gpt-5.4".into())))
        .unwrap();
    settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ReasoningEffort(Some("high".into())),
        )
        .unwrap();
    settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ServiceTier(Some("default".into())),
        )
        .unwrap();
    let payload = json!({
        "type":"response.create",
        "model":"gpt-5.6-sol",
        "reasoning":{"effort":"low","summary":"auto"},
        "service_tier":"priority"
    })
    .to_string();

    let rewritten = rewrite_request(
        &payload,
        RequestEnvelope::ResponseCreate,
        settings.thread_override(&thread),
        Some("priority"),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&rewritten.payload).unwrap();

    assert_eq!(value["model"], "gpt-5.4");
    assert_eq!(value["reasoning"]["effort"], "high");
    assert_eq!(value["reasoning"]["summary"], "auto");
    assert_eq!(value["service_tier"], "default");
    assert!(!rewritten.automatic_tier_applied);
}

#[test]
fn reasoning_object_is_created_when_missing() {
    let thread = ThreadId::new("thread");
    let mut settings = RotationSettings::default();
    settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ReasoningEffort(Some("ultra".into())),
        )
        .unwrap();

    let rewritten = rewrite_request(
        r#"{"model":"gpt-5.6-sol"}"#,
        RequestEnvelope::HttpResponses,
        settings.thread_override(&thread),
        None,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&rewritten.payload).unwrap();

    assert_eq!(value["reasoning"]["effort"], "ultra");
}
