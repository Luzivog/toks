use super::*;

#[test]
fn selector_recovers_unified_model_and_workspace_from_consistent_legacy_rows() {
    let mut legacy = test_message("covered", "grok:covered:0");
    legacy.model_id = "grok-4.5".to_string();
    legacy.set_workspace(
        Some("/tmp/project".to_string()),
        Some("project".to_string()),
    );
    let mut unified = test_message("covered", "grok-unified:covered:1:1:1");
    unified.model_id = UNKNOWN_MODEL.to_string();
    unified.workspace_key = None;
    unified.workspace_label = None;

    let messages = prefer_unified_log_messages(vec![legacy, unified]);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "grok-4.5");
    assert_eq!(messages[0].workspace_key.as_deref(), Some("/tmp/project"));
    assert_eq!(messages[0].workspace_label.as_deref(), Some("project"));
}

#[test]
fn selector_retains_uncovered_legacy_history_for_partially_unified_session() {
    let mut older_legacy = test_message("covered", "grok:covered:older");
    older_legacy.timestamp = 1_700_000_000_000;
    older_legacy.tokens.input = 10;

    let mut covered_legacy = test_message("covered", "grok:covered:covered");
    covered_legacy.timestamp = 1_700_000_001_000;
    covered_legacy.tokens.input = 20;

    let mut covered_unified = test_message("covered", "grok-unified:covered:event");
    covered_unified.timestamp = covered_legacy.timestamp;
    covered_unified.tokens.input = covered_legacy.tokens.input;

    let messages = prefer_unified_log_messages(vec![older_legacy, covered_legacy, covered_unified]);

    assert_eq!(messages.len(), 2);
    assert!(messages
        .iter()
        .any(|message| message.dedup_key.as_deref() == Some("grok:covered:older")));
    assert!(messages.iter().any(is_unified_log_message));
}

#[test]
fn selector_is_order_invariant_for_activity_and_fallback_rows() {
    let legacy_activity = test_message("covered", "grok:covered:usage:turn");
    let mut legacy_fallback = test_message("covered", "grok:covered:fallback");
    legacy_fallback.tokens.input = 10;
    let unified = test_message("covered", "grok-unified:covered:event");

    let first_order = prefer_unified_log_messages(vec![
        legacy_activity.clone(),
        legacy_fallback.clone(),
        unified.clone(),
    ]);
    let second_order = prefer_unified_log_messages(vec![legacy_fallback, legacy_activity, unified]);

    assert_eq!(first_order, second_order);
    assert_eq!(
        first_order
            .iter()
            .map(|message| message.dedup_key.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("grok-unified:covered:event")]
    );
}

#[test]
fn prefers_unified_log_messages_only_for_covered_sessions() {
    let covered_legacy = test_message("covered", "grok:covered:0");
    let uncovered_legacy = test_message("fallback", "grok:fallback:0");
    let covered_unified = test_message("covered", "grok-unified:covered:1:1:1");

    let messages =
        prefer_unified_log_messages(vec![covered_legacy, uncovered_legacy, covered_unified]);

    assert_eq!(messages.len(), 2);
    assert!(messages
        .iter()
        .any(|message| { message.session_id == "covered" && is_unified_log_message(message) }));
    assert!(messages
        .iter()
        .any(|message| message.session_id == "fallback"));
}
