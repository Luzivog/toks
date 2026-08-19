use tempfile::tempdir;
use toks_ingest::sessions::{CostSource, UnifiedMessage};
use toks_ingest::TokenBreakdown;

use super::{forgotten, reconcile_at};

fn observation(key: &str, timestamp: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        model_id: "gpt-test".into(),
        provider_id: "openai".into(),
        session_id: "session".into(),
        workspace_key: None,
        workspace_label: None,
        timestamp,
        date: "2026-02-02".into(),
        tokens: TokenBreakdown {
            input: 10,
            ..Default::default()
        },
        cost_source: CostSource::Estimated,
        cost: 0.0,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some(key.into()),
        durable_identity: None,
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}

#[test]
fn forgotten_range_removes_captured_events_and_blocks_replay() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let forgotten_event = observation("forgotten", 1_000);
    let retained = observation("retained", 2_000);
    reconcile_at(&path, &[forgotten_event.clone(), retained.clone()], 3_000).unwrap();

    assert_eq!(forgotten::forget_range(&path, 900, 1_500).unwrap(), 1);
    let replay = reconcile_at(
        &path,
        &[forgotten_event, retained, observation("new", 2_500)],
        4_000,
    )
    .unwrap();

    assert_eq!(replay.messages.len(), 2);
    assert!(replay
        .messages
        .iter()
        .all(|message| message.timestamp >= 1_500));
}
