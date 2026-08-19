use super::*;

#[test]
fn unified_message_without_identity_fields_uses_empty_defaults() {
    let message = UnifiedMessage::new(
        "legacy-client",
        "legacy-model",
        "legacy-provider",
        "legacy-session",
        1_700_000_000_000,
        crate::TokenBreakdown::default(),
        0.0,
    );
    let mut legacy_json = serde_json::to_value(message).unwrap();
    let object = legacy_json.as_object_mut().unwrap();
    object.remove("durable_identity");
    object.remove("accounting_aliases");

    let restored: UnifiedMessage = serde_json::from_value(legacy_json).unwrap();
    assert!(restored.durable_identity.is_none());
    assert!(restored.accounting_aliases.is_empty());
}
