use super::accounting_identity::CodexIdentityTracker;
use super::*;
use std::collections::BTreeMap;

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

#[test]
fn legacy_codex_occurrences_migrate_without_changing_the_next_identity() {
    let lineage = encode(&["parent-session", "child-session"]);
    let timestamp = "2026-01-01T00:00:01Z";
    let mut occurrences = BTreeMap::new();
    occurrences.insert(encode(&[&lineage, timestamp]), 2_u64);
    let legacy = serde_json::json!({
        "token_count_sequence": 2,
        "timestamp_occurrences": occurrences,
    });

    let mut restored: CodexIdentityTracker = serde_json::from_value(legacy).unwrap();
    let identity = restored.next(Some("parent-session"), "child-session", Some(timestamp));

    assert_eq!(
        identity.scheme,
        DurableIdentityScheme::CodexSessionTimestampOccurrence
    );
    assert_eq!(identity.version, 1);
    assert_eq!(identity.value, encode(&[&lineage, timestamp, "2"]));
    assert_eq!(identity.strength, IdentityStrength::SessionStable);
}

#[test]
fn compact_codex_occurrences_survive_restart_and_backward_duplicates() {
    let mut uninterrupted = CodexIdentityTracker::default();
    for timestamp in ["2026-01-01T00:00:02Z", "2026-01-01T00:00:03Z"] {
        uninterrupted.next(None, "logical-session", Some(timestamp));
    }
    let encoded = serde_json::to_vec(&uninterrupted).unwrap();
    let mut restarted: CodexIdentityTracker = serde_json::from_slice(&encoded).unwrap();

    let timestamp = "2026-01-01T00:00:02Z";
    let expected = uninterrupted.next(None, "logical-session", Some(timestamp));
    assert_eq!(
        restarted.next(None, "logical-session", Some(timestamp)),
        expected
    );
    assert_eq!(
        restarted.next(None, "logical-session", None),
        uninterrupted.next(None, "logical-session", None)
    );
}

#[test]
fn compact_codex_tracker_matches_the_legacy_identity_algorithm() {
    let cases = [
        (None, "session", Some("2026-01-01T00:00:02Z")),
        (None, "session", Some("2026-01-01T00:00:03Z")),
        (None, "session", Some("2026-01-01T00:00:02Z")),
        (Some("parent"), "child", Some("2026-01-01T00:00:02Z")),
        (Some("other-parent"), "child", Some("2026-01-01T00:00:02Z")),
        (Some("parent"), "child", Some("2026-01-01T00:00:02Z")),
        (None, "session", None),
        (None, "session", Some("")),
    ];
    let mut tracker = CodexIdentityTracker::default();
    let mut legacy_occurrences = BTreeMap::<String, u64>::new();
    for (sequence, (parent, session, timestamp)) in cases.into_iter().enumerate() {
        let lineage =
            parent.map_or_else(|| encode(&[session]), |parent| encode(&[parent, session]));
        let (scheme, value) = if let Some(timestamp) = timestamp.filter(|value| !value.is_empty()) {
            let key = encode(&[&lineage, timestamp]);
            let occurrence = legacy_occurrences.entry(key).or_default();
            let value = encode(&[&lineage, timestamp, &occurrence.to_string()]);
            *occurrence = occurrence.saturating_add(1);
            (
                DurableIdentityScheme::CodexSessionTimestampOccurrence,
                value,
            )
        } else {
            (
                DurableIdentityScheme::CodexSessionRecordSequence,
                encode(&[&lineage, &sequence.to_string()]),
            )
        };

        let actual = tracker.next(parent, session, timestamp);
        assert_eq!(actual.scheme, scheme);
        assert_eq!(actual.value, value);
        assert_eq!(actual.version, 1);
        assert_eq!(actual.strength, IdentityStrength::SessionStable);
    }
}

#[test]
fn fork_lineages_keep_independent_occurrence_counts_after_restart() {
    let timestamp = "2026-01-01T00:00:01Z";
    let mut tracker = CodexIdentityTracker::default();
    tracker.next(Some("parent-a"), "child", Some(timestamp));
    tracker.next(Some("parent-b"), "child", Some(timestamp));
    let mut restored: CodexIdentityTracker =
        serde_json::from_slice(&serde_json::to_vec(&tracker).unwrap()).unwrap();

    let second_a = restored.next(Some("parent-a"), "child", Some(timestamp));
    let second_b = restored.next(Some("parent-b"), "child", Some(timestamp));
    assert!(second_a.value.ends_with("|1:1"));
    assert!(second_b.value.ends_with("|1:1"));
    assert_ne!(second_a.value, second_b.value);
}

#[test]
fn compact_codex_state_is_linear_and_does_not_repeat_the_lineage() {
    let logical_session = "019c8c2a-7e21-7a81-81f4-b6e3b1a8815e";
    let lineage = encode(&[logical_session]);
    let mut tracker = CodexIdentityTracker::default();
    let mut legacy_occurrences = BTreeMap::new();
    for index in 0..250 {
        let timestamp = format!("2026-01-01T00:00:{index:06}Z");
        tracker.next(None, logical_session, Some(&timestamp));
        legacy_occurrences.insert(encode(&[&lineage, &timestamp]), 1_u64);
    }
    let small = serde_json::to_vec(&tracker).unwrap().len();
    for index in 250..500 {
        let timestamp = format!("2026-01-01T00:00:{index:06}Z");
        tracker.next(None, logical_session, Some(&timestamp));
        legacy_occurrences.insert(encode(&[&lineage, &timestamp]), 1_u64);
    }
    let compact = serde_json::to_vec(&tracker).unwrap();
    let legacy = serde_json::to_vec(&serde_json::json!({
        "token_count_sequence": 500,
        "timestamp_occurrences": legacy_occurrences,
    }))
    .unwrap();

    assert!(compact.len() - small < 250 * 40);
    assert!(compact.len() * 2 < legacy.len());
}

#[test]
fn compact_codex_tracker_round_trips_through_bincode_cache() {
    let mut tracker = CodexIdentityTracker::default();
    let first = tracker.next(None, "session-a", Some("2026-08-19T00:00:02Z"));
    let encoded = bincode::serialize(&tracker).unwrap();
    let mut restored: CodexIdentityTracker = bincode::deserialize(&encoded).unwrap();
    let next = restored.next(None, "session-a", Some("2026-08-19T00:00:02Z"));

    assert_ne!(first.value, next.value);
    assert!(next.value.ends_with("1:1"));
}

fn encode(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| format!("{}:{part}", part.len()))
        .collect::<Vec<_>>()
        .join("|")
}
