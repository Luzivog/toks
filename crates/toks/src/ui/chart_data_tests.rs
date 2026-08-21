use chrono::TimeZone;

use super::{
    all_time_points, all_time_summary, overview_usage_points, provider_rows, usage_chart_maximum,
    usage_chart_points, ProviderPoint,
};
use toks_core::{
    history::{UsageBucket, UsagePeriod, UsageSeries},
    DaySlice, HistorySnapshot, SourceHistory,
};

#[test]
fn tooltip_hides_zero_providers_and_sorts_by_cost() {
    let point = ProviderPoint {
        heading: "August 18".into(),
        label: "08-18".into(),
        claude: 8.0,
        claude_tokens: 80,
        codex: 3.0,
        codex_tokens: 30,
        opencode: 5.0,
        opencode_tokens: 50,
    };
    let rows = provider_rows(&point);
    assert_eq!(rows[0].0, "Claude Code");
    assert_eq!(rows[1].0, "OpenCode");
    assert_eq!(rows[2].0, "Codex");

    let codex_only = ProviderPoint {
        claude: 0.0,
        claude_tokens: 0,
        opencode: 0.0,
        opencode_tokens: 0,
        ..point
    };
    let rows = provider_rows(&codex_only);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Codex");
}

#[test]
fn usage_chart_scale_is_driven_by_tokens_instead_of_cost() {
    let points = [
        ProviderPoint {
            heading: "Expensive".into(),
            label: "1".into(),
            claude: 10_000.0,
            claude_tokens: 10,
            codex: 0.0,
            codex_tokens: 0,
            opencode: 0.0,
            opencode_tokens: 500,
        },
        ProviderPoint {
            heading: "Token heavy".into(),
            label: "2".into(),
            claude: 1.0,
            claude_tokens: 1_000,
            codex: 0.0,
            codex_tokens: 0,
            opencode: 0.0,
            opencode_tokens: 0,
        },
    ];
    assert_eq!(usage_chart_maximum(&points), 1_000.0);
}

#[test]
fn page_charts_stop_at_the_current_hour_and_day() {
    let history = HistorySnapshot {
        sources: vec![SourceHistory {
            client: "codex".into(),
            days: vec![DaySlice {
                date: "2026-08-18".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        generated_at_ms: chrono::Local
            .with_ymd_and_hms(2026, 8, 18, 12, 34, 0)
            .single()
            .unwrap()
            .timestamp_millis(),
        ..Default::default()
    };

    assert_eq!(usage_chart_points(&history, UsagePeriod::Hourly).len(), 60);
    assert_eq!(usage_chart_points(&history, UsagePeriod::Daily).len(), 13);
    assert_eq!(usage_chart_points(&history, UsagePeriod::Monthly).len(), 18);
    assert_eq!(overview_usage_points(&history).len(), 30);
}

#[test]
fn overview_joins_provider_days_by_key_instead_of_position() {
    let source = |client: &str, key: &str, cost: f64| SourceHistory {
        client: client.into(),
        days: vec![DaySlice {
            date: "2026-08-18".into(),
            ..Default::default()
        }],
        usage: UsageSeries {
            daily: vec![UsageBucket {
                key: key.into(),
                cost,
                tokens: (cost * 10.0) as i64,
                ..Default::default()
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let history = HistorySnapshot {
        sources: vec![
            source("claude", "2026-08-18", 8.0),
            source("codex", "2026-08-17", 3.0),
            source("opencode", "2026-08-18", 2.0),
        ],
        ..Default::default()
    };

    let points = overview_usage_points(&history);
    let previous = points.iter().find(|point| point.label == "08-17").unwrap();
    let current = points.iter().find(|point| point.label == "08-18").unwrap();
    assert_eq!((previous.claude, previous.codex), (0.0, 3.0));
    assert_eq!((current.claude, current.codex), (8.0, 0.0));
    assert_eq!((current.opencode, current.opencode_tokens), (2.0, 20));
}

#[test]
fn all_time_chart_aggregates_weeks_and_summary_uses_exact_totals() {
    let daily_bucket = |key: &str, cost: f64, tokens: i64| UsageBucket {
        key: key.into(),
        cost,
        tokens,
        ..Default::default()
    };
    let source =
        |client: &str, total_cost: f64, total_tokens: i64, daily: Vec<UsageBucket>| SourceHistory {
            client: client.into(),
            total_cost,
            total_tokens,
            usage: UsageSeries {
                daily,
                ..Default::default()
            },
            ..Default::default()
        };
    let history = HistorySnapshot {
        sources: vec![
            source(
                "claude",
                90.0,
                900,
                vec![
                    daily_bucket("2026-01-06", 1.0, 10),
                    daily_bucket("2026-01-07", 2.0, 20),
                ],
            ),
            source(
                "codex",
                40.0,
                400,
                vec![daily_bucket("2026-01-20", 4.0, 40)],
            ),
            source(
                "opencode",
                20.0,
                200,
                vec![daily_bucket("2026-01-07", 5.0, 50)],
            ),
        ],
        usage: UsageSeries {
            daily: vec![
                daily_bucket("2026-01-06", 1.0, 10),
                daily_bucket("2026-01-07", 2.0, 20),
                daily_bucket("2026-01-20", 4.0, 40),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let points = all_time_points(&history);
    assert_eq!(
        points
            .iter()
            .map(|point| point.label.as_str())
            .collect::<Vec<_>>(),
        ["Jan 5", "Jan 12", "Jan 19"]
    );
    assert_eq!(points[0].heading.as_str(), "January 5–11, 2026");
    assert_eq!((points[0].claude, points[0].claude_tokens), (3.0, 30));
    assert_eq!((points[0].opencode, points[0].opencode_tokens), (5.0, 50));
    assert_eq!(points[1].claude_tokens + points[1].codex_tokens, 0);
    assert_eq!((points[2].codex, points[2].codex_tokens), (4.0, 40));

    let summary = all_time_summary(&history);
    assert_eq!(summary.claude_cost, 90.0);
    assert_eq!(summary.claude_tokens, 900);
    assert_eq!(summary.codex_cost, 40.0);
    assert_eq!(summary.codex_tokens, 400);
    assert_eq!(summary.opencode_cost, 20.0);
    assert_eq!(summary.opencode_tokens, 200);
}
