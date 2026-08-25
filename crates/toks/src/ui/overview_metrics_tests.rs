use chrono::NaiveDate;
use toks_core::history::{CostCoverage, UsageBucket, UsageSeries};

use super::overview_metrics::week_to_date_bucket;

#[test]
fn week_to_date_starts_on_monday_and_stops_at_today() {
    let usage = UsageSeries {
        daily: vec![
            bucket("2026-08-16", 1),
            bucket("2026-08-17", 2),
            bucket("2026-08-18", 3),
            bucket("2026-08-19", 4),
            bucket("malformed", 100),
        ],
        ..Default::default()
    };

    let total = week_to_date_bucket(
        &usage,
        NaiveDate::from_ymd_opt(2026, 8, 18).expect("valid Tuesday"),
    );

    assert_eq!(total.key, "2026-08-17");
    assert_eq!(total.turns, 35);
    assert_eq!(total.messages, 30);
    assert_eq!(total.input, 5);
    assert_eq!(total.output, 10);
    assert_eq!(total.reasoning, 25);
    assert_eq!(total.cache_read, 15);
    assert_eq!(total.cache_write, 20);
    assert_eq!(total.tokens, 75);
    assert_eq!(total.cost, 2.5);
    assert_eq!(total.cost_coverage.covered_tokens, 50);
    assert_eq!(total.cost_coverage.uncovered_messages, 5);
}

fn bucket(key: &str, value: i64) -> UsageBucket {
    UsageBucket {
        key: key.into(),
        input: value,
        output: value * 2,
        cache_read: value * 3,
        cache_write: value * 4,
        reasoning: value * 5,
        tokens: value * 15,
        messages: value * 6,
        turns: value * 7,
        cost: value as f64 / 2.0,
        cost_coverage: CostCoverage {
            covered_tokens: value * 10,
            uncovered_messages: value,
            ..Default::default()
        },
        ..Default::default()
    }
}
