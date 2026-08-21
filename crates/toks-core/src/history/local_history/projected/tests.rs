use chrono::TimeZone;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::basis::PricingBasis;

use super::{snapshot, ArchiveProjection, ArchiveRollup, RollupPeriod};

#[test]
fn opencode_rollups_project_into_their_own_source() {
    let timestamp = chrono::Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 30, 0)
        .single()
        .unwrap();
    let rollup = |client: &str| ArchiveRollup {
        period: RollupPeriod::All,
        bucket_start_ms: 0,
        client: client.into(),
        provider: "openai".into(),
        model: "gpt-test".into(),
        cost_source: 0,
        long_context: false,
        input: 10,
        output: 2,
        cache_read: 3,
        cache_write: 0,
        reasoning: 1,
        messages: 1,
        turns: 1,
        cost_nanos: 0,
        event_count: 1,
        pricing_basis: PricingBasis::default(),
    };
    let projection = ArchiveProjection {
        rollups: vec![rollup("opencode"), rollup("claude"), rollup("unknown")],
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
