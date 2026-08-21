use chrono::{TimeZone, Utc};
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::sessions::{CostSource, UnifiedMessage};
use toks_ingest::TokenBreakdown;

use super::archive::ArchiveCapture;
use super::materialize;

#[test]
fn archived_utc_facts_rebuild_calendar_views_and_capture_provenance() {
    let timestamp = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let capture = ArchiveCapture {
        messages: vec![message(timestamp.timestamp_millis())],
        captured_since_ms: Some(100),
        captured_through_ms: Some(200),
        strong_events: 1,
        ..Default::default()
    };

    let snapshot = materialize::snapshot(
        capture,
        timestamp,
        &BucketTimezone::from_pinned_name(Some("UTC")),
        None,
    );

    assert_eq!(snapshot.captured_since_ms, Some(100));
    assert_eq!(snapshot.captured_through_ms, Some(200));
    assert_eq!(snapshot.strong_events, 1);
    assert_eq!(snapshot.sources[0].total_tokens, 16);
    assert_eq!(snapshot.sources[0].total_cost, 2.5);
    assert_eq!(snapshot.usage.daily[0].key, "2026-08-18");
    assert_eq!(snapshot.usage.monthly[0].key, "2026-08");
}

#[test]
fn estimated_cost_is_a_projection_not_a_durable_fact() {
    let timestamp = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let mut observation = message(timestamp.timestamp_millis());
    observation.cost = 99.0;
    observation.cost_source = CostSource::Estimated;
    let snapshot = materialize::snapshot(
        ArchiveCapture {
            messages: vec![observation],
            ..Default::default()
        },
        timestamp,
        &BucketTimezone::from_pinned_name(Some("UTC")),
        None,
    );

    assert_eq!(snapshot.sources[0].total_cost, 0.0);
    assert!(snapshot.unpriced);
}

#[test]
fn claude_mirror_usage_joins_the_claude_projection() {
    let timestamp = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let mut observation = message(timestamp.timestamp_millis());
    observation.client = "cc-mirror/team".into();
    let snapshot = materialize::snapshot(
        ArchiveCapture {
            messages: vec![observation],
            ..Default::default()
        },
        timestamp,
        &BucketTimezone::from_pinned_name(Some("UTC")),
        None,
    );

    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.sources[0].client, "claude");
    assert_eq!(snapshot.sources[0].total_tokens, 16);
}

#[test]
fn opencode_observations_form_their_own_source() {
    let timestamp = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let mut observation = message(timestamp.timestamp_millis());
    observation.client = "opencode".into();
    observation.model_id = "opencode-model".into();
    let codex = message(timestamp.timestamp_millis());
    let snapshot = materialize::snapshot(
        ArchiveCapture {
            messages: vec![observation, codex],
            ..Default::default()
        },
        timestamp,
        &BucketTimezone::from_pinned_name(Some("UTC")),
        None,
    );

    assert_eq!(snapshot.sources.len(), 2);
    let opencode = snapshot
        .sources
        .iter()
        .find(|source| source.client == "opencode")
        .unwrap();
    assert_eq!(opencode.total_tokens, 16);
    assert_eq!(opencode.total_cost, 2.5);
    assert!(snapshot.usage.daily[0].tokens > 16);
}

fn message(timestamp: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        model_id: "gpt-test".into(),
        provider_id: "openai".into(),
        session_id: "opaque".into(),
        workspace_key: None,
        workspace_label: None,
        timestamp,
        date: String::new(),
        tokens: TokenBreakdown {
            input: 10,
            output: 2,
            cache_read: 3,
            cache_write: 0,
            reasoning: 1,
        },
        cost: 2.5,
        cost_source: CostSource::ProviderReported,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some("event-1".into()),
        durable_identity: None,
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}
