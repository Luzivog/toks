use chrono::{TimeZone, Utc};
use tokscope_ingest::bucket_tz::BucketTimezone;
use tokscope_ingest::sessions::{CostSource, UnifiedMessage};
use tokscope_ingest::TokenBreakdown;

use super::ingress::ValidatedMessage;
use super::rollup::{merge_usage_series, UsageRollup};
use super::{
    ModelUsage, SourceHistory, UsageBucket, UsageKey, UsagePeriod, UsageRange, UsageSeries,
};

fn message(hour: u32, turns: bool) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        model_id: "gpt-test".into(),
        provider_id: "openai".into(),
        session_id: format!("session-{hour}"),
        workspace_key: None,
        workspace_label: None,
        timestamp: Utc
            .with_ymd_and_hms(2026, 8, 18, hour, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis(),
        date: "2026-08-18".into(),
        tokens: TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 20,
            cache_write: 2,
            reasoning: 3,
        },
        cost: 1.25,
        cost_source: CostSource::Estimated,
        duration_ms: None,
        message_count: 2,
        agent: None,
        dedup_key: None,
        session_title: None,
        is_turn_start: turns,
        model_attribution_conflicted: false,
    }
}

#[test]
fn usage_rollup_builds_all_periods_from_the_same_messages() {
    let timezone = BucketTimezone::from_pinned_name(Some("UTC"));
    let mut rollup = UsageRollup::default();
    let first = message(9, true);
    let second = message(11, false);
    rollup.add(&ValidatedMessage::new(&first), &timezone);
    rollup.add(&ValidatedMessage::new(&second), &timezone);

    let usage = rollup.finish();
    assert_eq!(usage.daily.len(), 1);
    assert_eq!(usage.hourly.len(), 2);
    assert_eq!(usage.monthly.len(), 1);

    let day = &usage.daily[0];
    assert_eq!(day.key, "2026-08-18");
    assert_eq!(day.input, 20);
    assert_eq!(day.cache_read, 40);
    assert_eq!(day.tokens, 80);
    assert_eq!(day.messages, 4);
    assert_eq!(day.turns, 1);
    assert_eq!(day.cost, 2.5);
    assert_eq!(day.models.len(), 1);
    assert_eq!(day.models[0].model, "gpt-test");
    assert_eq!(day.models[0].tokens, 80);
    assert_eq!(usage.monthly[0].key, "2026-08");
}

#[test]
fn provider_series_merge_without_reprocessing_messages() {
    let source = |client: &str, tokens: i64, cost: f64| SourceHistory {
        client: client.to_string(),
        usage: UsageSeries {
            daily: vec![UsageBucket {
                key: "2026-08-18".into(),
                input: tokens,
                tokens,
                cost,
                models: vec![ModelUsage {
                    model: "shared-model".into(),
                    provider: client.into(),
                    input: tokens,
                    tokens,
                    cost,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
        ..Default::default()
    };

    let merged = merge_usage_series(&[source("claude", 20, 2.0), source("codex", 30, 3.0)]);
    assert_eq!(merged.daily.len(), 1);
    assert_eq!(merged.daily[0].tokens, 50);
    assert_eq!(merged.daily[0].cost, 5.0);
    assert_eq!(merged.daily[0].models.len(), 2);
}

#[test]
fn ingress_clamps_invalid_metrics_and_marks_cost_uncovered() {
    let timezone = BucketTimezone::from_pinned_name(Some("UTC"));
    let mut source = message(9, true);
    source.tokens.input = -10;
    source.tokens.output = 5;
    source.message_count = -2;
    source.cost = f64::NAN;
    let mut rollup = UsageRollup::default();
    rollup.add(&ValidatedMessage::new(&source), &timezone);

    let bucket = &rollup.finish().daily[0];
    assert_eq!(bucket.input, 0);
    assert_eq!(bucket.output, 5);
    assert_eq!(bucket.messages, 0);
    assert_eq!(bucket.cost, 0.0);
    assert_eq!(bucket.cost_coverage.uncovered_tokens, 30);
    assert_eq!(bucket.cost_coverage.invalid_records, 1);
}

#[test]
fn provider_reported_cost_is_covered_without_pricing_service_state() {
    let timezone = BucketTimezone::from_pinned_name(Some("UTC"));
    let mut source = message(9, false);
    source.cost_source = CostSource::ProviderReported;
    let mut rollup = UsageRollup::default();
    rollup.add(&ValidatedMessage::new(&source), &timezone);

    let coverage = rollup.finish().daily[0].cost_coverage;
    assert!(coverage.is_complete());
    assert_eq!(coverage.covered_tokens, 40);
    assert_eq!(coverage.covered_ratio(), 1.0);
}

#[test]
fn typed_ranges_reject_mixed_periods_and_query_inclusively() {
    let start = UsageKey::parse(UsagePeriod::Daily, "2026-08-01").unwrap();
    let end = UsageKey::parse(UsagePeriod::Daily, "2026-08-03").unwrap();
    let range = UsageRange::new(start, end).unwrap();
    let usage = UsageSeries {
        daily: ["2026-07-31", "2026-08-01", "2026-08-03", "bad"]
            .into_iter()
            .map(|key| UsageBucket {
                key: key.into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    let keys: Vec<_> = usage
        .query(range)
        .into_iter()
        .map(|b| b.key.as_str())
        .collect();
    assert_eq!(keys, ["2026-08-01", "2026-08-03"]);
    let month = UsageKey::parse(UsagePeriod::Monthly, "2026-08").unwrap();
    assert!(UsageRange::new(start, month).is_none());
}

#[test]
fn trailing_month_range_crosses_year_boundary() {
    let end = UsageKey::parse(UsagePeriod::Monthly, "2026-02").unwrap();
    let range = UsageRange::trailing(end, 4).unwrap();
    assert_eq!(range.start.to_string(), "2025-11");
    assert_eq!(range.end.to_string(), "2026-02");
}
