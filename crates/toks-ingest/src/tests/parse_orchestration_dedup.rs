use super::*;
fn make_workbuddy_message(
    session_id: &str,
    timestamp: i64,
    input: i64,
    dedup_key: &str,
) -> UnifiedMessage {
    let mut msg = UnifiedMessage::new(
        "workbuddy",
        "glm-5.2",
        "zai",
        session_id,
        timestamp,
        TokenBreakdown {
            input,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );
    msg.dedup_key = Some(dedup_key.to_string());
    msg
}

fn make_trae_message(
    session_id: &str,
    timestamp: i64,
    dedup_key: Option<&str>,
    cost: f64,
) -> UnifiedMessage {
    UnifiedMessage::new_with_dedup(
        "trae",
        "gpt-5.2",
        "openai",
        session_id,
        timestamp,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        cost,
        dedup_key.map(str::to_string),
    )
}

#[test]
fn workbuddy_fallback_dedups_by_session_not_date() {
    const DAY1: i64 = 1_782_883_200_000;
    const DAY2: i64 = 1_782_969_600_000;

    // Session A has detailed coverage on DAY1.
    let detailed = vec![make_workbuddy_message(
        "sess-A",
        DAY1,
        100,
        "workbuddy:detailed-A",
    )];
    let fallback = vec![
        // Session A's cumulative SQLite aggregate is dated DAY2 (updated_at)
        // even though its detailed activity was DAY1. The old date-overlap
        // check kept it, double-counting the whole session on DAY2.
        make_workbuddy_message("sess-A", DAY2, 5000, "workbuddy:fallback-A"),
        // Session B has NO detailed coverage but its aggregate shares DAY1
        // with session A's detail. The old check dropped it, losing usage.
        make_workbuddy_message("sess-B", DAY1, 2000, "workbuddy:fallback-B"),
    ];

    let merged = crate::merge_workbuddy_messages(detailed, fallback);

    // Detailed A kept; fallback A dropped (session covered); fallback B kept.
    assert_eq!(merged.len(), 2);
    assert!(merged
        .iter()
        .any(|message| message.dedup_key.as_deref() == Some("workbuddy:detailed-A")));
    assert!(merged
        .iter()
        .any(|message| message.dedup_key.as_deref() == Some("workbuddy:fallback-B")));
    assert!(!merged
        .iter()
        .any(|message| message.dedup_key.as_deref() == Some("workbuddy:fallback-A")));
}

#[test]
fn workbuddy_fallback_kept_when_no_detailed_messages() {
    // With zero detailed coverage, every fallback session survives.
    let fallback = vec![
        make_workbuddy_message("sess-A", 1_782_883_200_000, 1000, "workbuddy:fallback-A"),
        make_workbuddy_message("sess-B", 1_782_969_600_000, 2000, "workbuddy:fallback-B"),
    ];

    let merged = crate::merge_workbuddy_messages(Vec::new(), fallback);

    assert_eq!(merged.len(), 2);
}

#[test]
fn test_dedupe_latest_trae_messages_keeps_latest_timestamp_for_session() {
    let messages = vec![
        make_trae_message(
            "session-stable",
            1_700_000_002_000,
            Some("trae:session-stable:1_700_000_002"),
            0.2,
        ),
        make_trae_message(
            "session-stable",
            1_700_000_003_000,
            Some("trae:session-stable:1_700_000_003"),
            0.3,
        ),
        make_trae_message(
            "session-other",
            1_700_000_001_000,
            Some("trae:session-other:1_700_000_001"),
            0.1,
        ),
    ];

    let deduped = dedupe_latest_trae_messages(messages);

    assert_eq!(deduped.len(), 2);
    let stable = deduped
        .iter()
        .find(|msg| msg.session_id == "session-stable")
        .expect("session-stable should remain after dedupe");
    assert_eq!(stable.timestamp, 1_700_000_003_000);
    assert_eq!(stable.cost, 0.3);
    assert_eq!(
        stable.dedup_key.as_deref(),
        Some("trae:session-stable:1_700_000_003")
    );
}

#[test]
fn test_dedupe_latest_trae_messages_tiebreaks_by_dedup_key() {
    let messages = vec![
        make_trae_message(
            "session-stable",
            1_700_000_010_000,
            Some("dedupe-key-a"),
            0.2,
        ),
        make_trae_message(
            "session-stable",
            1_700_000_010_000,
            Some("dedupe-key-z"),
            0.4,
        ),
        make_trae_message(
            "session-stable",
            1_700_000_009_000,
            Some("dedupe-key-m"),
            0.1,
        ),
    ];

    let deduped = dedupe_latest_trae_messages(messages);

    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].timestamp, 1_700_000_010_000);
    assert_eq!(deduped[0].dedup_key.as_deref(), Some("dedupe-key-z"));
    assert_eq!(deduped[0].cost, 0.4);
}
