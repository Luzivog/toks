use chrono::TimeZone;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::basis::PricingBasis;

use super::{snapshot, ProjectionBuilder};
use crate::history::archive::{ArchiveProjection, ArchiveRollup, RollupPeriod};

fn rollup(period: RollupPeriod, client: &str, bucket_start_ms: i64, input: i64) -> ArchiveRollup {
    ArchiveRollup {
        period,
        bucket_start_ms,
        client: client.into(),
        provider: "openai".into(),
        model: "gpt-test".into(),
        cost_source: 0,
        long_context: false,
        input,
        output: 2,
        cache_read: 3,
        cache_write: 0,
        reasoning: 1,
        messages: 1,
        turns: 1,
        cost_nanos: 0,
        event_count: 1,
        pricing_basis: PricingBasis::default(),
    }
}

#[test]
fn opencode_rollups_project_into_their_own_source() {
    let timestamp = chrono::Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let projection = ArchiveProjection {
        rollups: vec![
            rollup(RollupPeriod::All, "opencode", 0, 10),
            rollup(RollupPeriod::All, "claude", 0, 10),
            rollup(RollupPeriod::All, "unknown", 0, 10),
        ],
        projection_complete: true,
        ..Default::default()
    };

    let snapshot = snapshot(
        projection,
        timestamp,
        &BucketTimezone::from_pinned_name(Some("UTC")),
        None,
    );

    let clients: Vec<_> = snapshot.sources.iter().map(|s| s.client.as_str()).collect();
    assert_eq!(clients, ["claude", "opencode"]);
    let opencode = &snapshot.sources[1];
    assert_eq!(opencode.total_tokens, 16);
}

#[test]
fn source_series_keep_every_period_needed_for_provider_filtering() {
    let now = chrono::Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let timestamp = |year, month, day, hour, minute| {
        chrono::Utc
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap()
            .timestamp_millis()
    };
    let mut all = rollup(RollupPeriod::All, "codex", 0, 60);
    all.output = 6;
    all.cache_read = 9;
    all.reasoning = 3;
    all.messages = 3;
    all.turns = 3;
    all.event_count = 3;
    let projection = ArchiveProjection {
        rollups: vec![
            all,
            rollup(
                RollupPeriod::Minute,
                "codex",
                timestamp(2025, 4, 10, 7, 0),
                10,
            ),
            rollup(
                RollupPeriod::Minute,
                "codex",
                timestamp(2026, 8, 18, 8, 15),
                20,
            ),
            rollup(
                RollupPeriod::Minute,
                "codex",
                timestamp(2026, 8, 18, 12, 29),
                30,
            ),
        ],
        projection_complete: true,
        ..Default::default()
    };

    let snapshot = snapshot(
        projection,
        now,
        &BucketTimezone::from_pinned_name(Some("UTC")),
        None,
    );
    let source = snapshot.source("codex").unwrap();

    assert_eq!(source.total_tokens, 78);
    assert_eq!(source.usage.daily.len(), 2);
    assert_eq!(source.usage.hourly.len(), 3);
    assert_eq!(source.usage.hourly[0].key, "2025-04-10 07:00");
    assert_eq!(source.usage.monthly.len(), 2);
    assert_eq!(source.usage.monthly[0].key, "2025-04");
    assert_eq!(source.usage.monthly[1].key, "2026-08");
    assert_eq!(
        source
            .minutes
            .iter()
            .filter(|slice| slice.tokens > 0)
            .count(),
        1
    );
    assert_eq!(snapshot.usage.hourly.len(), 3);
    assert_eq!(snapshot.usage.daily.len(), 2);
    assert_eq!(snapshot.usage.monthly.len(), 2);
    assert_eq!(
        snapshot
            .usage
            .daily
            .iter()
            .map(|bucket| bucket.tokens)
            .sum::<i64>(),
        78
    );
}

#[test]
fn streaming_builder_preserves_named_timezone_bucket_boundaries() {
    let utc = |year, month, day, hour, minute| {
        chrono::Utc
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap()
    };
    let timezone = BucketTimezone::from_pinned_name(Some("America/New_York"));
    let now = utc(2026, 1, 1, 12, 0);
    let rows = [
        rollup(
            RollupPeriod::Minute,
            "codex",
            utc(2026, 1, 1, 0, 30).timestamp_millis(),
            10,
        ),
        rollup(
            RollupPeriod::Minute,
            "codex",
            utc(2026, 1, 1, 5, 30).timestamp_millis(),
            20,
        ),
    ];
    let mut builder = ProjectionBuilder::new(now, &timezone, None);
    for row in &rows {
        builder.add(row);
    }

    let snapshot = builder.finish(ArchiveProjection {
        projection_complete: true,
        ..Default::default()
    });

    let daily: Vec<_> = snapshot
        .usage
        .daily
        .iter()
        .map(|bucket| bucket.key.as_str())
        .collect();
    let monthly: Vec<_> = snapshot
        .usage
        .monthly
        .iter()
        .map(|bucket| bucket.key.as_str())
        .collect();
    assert_eq!(daily, ["2025-12-31", "2026-01-01"]);
    assert_eq!(monthly, ["2025-12", "2026-01"]);
    assert_eq!(snapshot.usage.hourly[0].key, "2025-12-31 19:00");
    assert_eq!(snapshot.usage.hourly[1].key, "2026-01-01 00:00");
    let source = snapshot.source("codex").unwrap();
    assert_eq!(source.usage.hourly[0].key, "2025-12-31 19:00");
    assert_eq!(source.usage.hourly[1].key, "2026-01-01 00:00");
    assert_eq!(source.usage.monthly[0].key, "2025-12");
    assert_eq!(source.usage.monthly[1].key, "2026-01");
}
