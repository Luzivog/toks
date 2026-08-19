use super::{
    aggregate_model_usage, all_time_points, all_time_summary, fmt_age, fmt_cost_per_million,
    hourly_bucket_day, overview_usage_points, provider_rows, sort_model_usage, sort_usage_buckets,
    split_limit_label, usage_bucket_is_current, usage_bucket_label, usage_chart_maximum,
    usage_chart_points, usage_hover_geometry, usage_marker_top, visible_usage_buckets,
    visible_usage_row_count, ProviderPoint,
};
use crate::{ModelSortColumn, ModelTablesState, Page, SortDirection, SortState, UsageSortColumn};
use chrono::{Local, NaiveDate, TimeZone, Utc};
use toks_core::{
    history::{ModelUsage, UsageBucket, UsagePeriod, UsageSeries},
    DaySlice, HistorySnapshot, SourceHistory,
};

#[test]
fn hover_regions_snap_at_midpoints_between_points() {
    assert_eq!(usage_hover_geometry(0, 3), (0.0, 0.25, 0.0));
    assert_eq!(usage_hover_geometry(1, 3), (0.25, 0.5, 0.5));
    assert_eq!(usage_hover_geometry(2, 3), (0.75, 0.25, 1.0));
}

#[test]
fn plan_limit_labels_separate_the_window_from_its_scope() {
    assert_eq!(
        split_limit_label("Weekly — GPT-5.3-Codex-Spark"),
        ("Weekly", Some("GPT-5.3-Codex-Spark"))
    );
    assert_eq!(
        split_limit_label("Weekly (all models)"),
        ("Weekly", Some("All models"))
    );
    assert_eq!(split_limit_label("Session"), ("Session", None));
}

#[test]
fn account_freshness_uses_compact_relative_time() {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 19, 15, 0)
        .single()
        .unwrap();
    assert_eq!(fmt_age(now, now), "just now");
    assert_eq!(fmt_age(now, now - chrono::Duration::minutes(7)), "7m ago");
    assert_eq!(fmt_age(now, now - chrono::Duration::hours(3)), "3h ago");
}

#[test]
fn marker_position_matches_chart_scale() {
    let highest = usage_marker_top(100.0, 100.0);
    let baseline = usage_marker_top(0.0, 100.0);
    assert_eq!(highest, 0.0);
    assert_eq!(baseline, 1.0);
}

#[test]
fn tooltip_hides_zero_providers_and_sorts_by_cost() {
    let point = ProviderPoint {
        heading: "August 18".into(),
        label: "08-18".into(),
        claude: 8.0,
        claude_tokens: 80,
        codex: 3.0,
        codex_tokens: 30,
    };
    let rows = provider_rows(&point);
    assert_eq!(rows[0].0, "Claude Code");
    assert_eq!(rows[1].0, "Codex");

    let codex_only = ProviderPoint {
        claude: 0.0,
        claude_tokens: 0,
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
        },
        ProviderPoint {
            heading: "Token heavy".into(),
            label: "2".into(),
            claude: 1.0,
            claude_tokens: 1_000,
            codex: 0.0,
            codex_tokens: 0,
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
        generated_at_ms: Local
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
fn aggregate_cost_per_million_is_clear_and_zero_safe() {
    assert_eq!(fmt_cost_per_million(2.5, 5_000_000), "$0.50");
    assert_eq!(fmt_cost_per_million(0.0, 0), "—");
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
        ],
        ..Default::default()
    };

    let points = overview_usage_points(&history);
    let previous = points.iter().find(|point| point.label == "08-17").unwrap();
    let current = points.iter().find(|point| point.label == "08-18").unwrap();
    assert_eq!((previous.claude, previous.codex), (0.0, 3.0));
    assert_eq!((current.claude, current.codex), (8.0, 0.0));
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
    assert_eq!(points[1].claude_tokens + points[1].codex_tokens, 0);
    assert_eq!((points[2].codex, points[2].codex_tokens), (4.0, 40));

    let summary = all_time_summary(&history);
    assert_eq!(summary.claude_cost, 90.0);
    assert_eq!(summary.claude_tokens, 900);
    assert_eq!(summary.codex_cost, 40.0);
    assert_eq!(summary.codex_tokens, 400);
}

#[test]
fn hourly_rows_use_time_labels_and_stable_day_groups() {
    assert_eq!(
        usage_bucket_label(UsagePeriod::Hourly, "2026-08-18 17:00"),
        "17:00"
    );
    assert_eq!(
        hourly_bucket_day("2026-08-18 17:00"),
        NaiveDate::from_ymd_opt(2026, 8, 18)
    );
    assert_eq!(
        hourly_bucket_day("2026-08-18 02:00"),
        hourly_bucket_day("2026-08-18 17:00")
    );
    assert_ne!(
        hourly_bucket_day("2026-08-17 23:00"),
        hourly_bucket_day("2026-08-18 00:00")
    );
}

#[test]
fn usage_tables_start_at_ten_rows_and_page_by_fifty() {
    assert_eq!(visible_usage_row_count(4, 10), 4);
    assert_eq!(visible_usage_row_count(80, 10), 10);
    assert_eq!(visible_usage_row_count(80, 60), 60);
}

#[test]
fn only_current_daily_and_monthly_rows_are_highlighted() {
    assert!(usage_bucket_is_current(
        UsagePeriod::Daily,
        "2026-08-18",
        "2026-08-18"
    ));
    assert!(usage_bucket_is_current(
        UsagePeriod::Monthly,
        "2026-08",
        "2026-08"
    ));
    assert!(!usage_bucket_is_current(
        UsagePeriod::Hourly,
        "2026-08-18 17:00",
        "2026-08-18 17:00"
    ));
    assert!(!usage_bucket_is_current(
        UsagePeriod::Daily,
        "2026-08-17",
        "2026-08-18"
    ));
}

#[test]
fn usage_metrics_sort_globally_in_both_directions() {
    let low = UsageBucket {
        key: "2026-08-18 10:00".into(),
        tokens: 10,
        ..Default::default()
    };
    let high = UsageBucket {
        key: "2026-08-18 09:00".into(),
        tokens: 90,
        ..Default::default()
    };
    let mut rows = vec![&low, &high];
    sort_usage_buckets(
        &mut rows,
        SortState {
            column: Some(UsageSortColumn::Total),
            direction: SortDirection::Descending,
        },
    );
    assert_eq!(rows[0].tokens, 90);

    sort_usage_buckets(
        &mut rows,
        SortState {
            column: Some(UsageSortColumn::Total),
            direction: SortDirection::Ascending,
        },
    );
    assert_eq!(rows[0].tokens, 10);
}

#[test]
fn usage_cost_per_million_sorts_across_the_full_result_set() {
    let low = UsageBucket {
        key: "2025-01-01".into(),
        tokens: 2_000_000,
        cost: 1.0,
        ..Default::default()
    };
    let high = UsageBucket {
        key: "2026-01-01".into(),
        tokens: 1_000_000,
        cost: 3.0,
        ..Default::default()
    };
    let mut rows = vec![&low, &high];
    sort_usage_buckets(
        &mut rows,
        SortState {
            column: Some(UsageSortColumn::CostPerMillion),
            direction: SortDirection::Descending,
        },
    );
    assert_eq!(rows[0].key, "2026-01-01");
}

#[test]
fn hourly_period_sort_orders_days_and_their_hours_together() {
    let buckets = [
        UsageBucket {
            key: "2026-08-17 23:00".into(),
            ..Default::default()
        },
        UsageBucket {
            key: "2026-08-18 01:00".into(),
            ..Default::default()
        },
        UsageBucket {
            key: "2026-08-18 09:00".into(),
            ..Default::default()
        },
    ];
    let mut rows = buckets.iter().collect::<Vec<_>>();
    sort_usage_buckets(
        &mut rows,
        SortState {
            column: Some(UsageSortColumn::Period),
            direction: SortDirection::Descending,
        },
    );
    assert_eq!(
        rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
        ["2026-08-18 09:00", "2026-08-18 01:00", "2026-08-17 23:00",]
    );
    sort_usage_buckets(
        &mut rows,
        SortState {
            column: Some(UsageSortColumn::Period),
            direction: SortDirection::Ascending,
        },
    );
    assert_eq!(
        rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
        ["2026-08-17 23:00", "2026-08-18 01:00", "2026-08-18 09:00",]
    );
}

#[test]
fn model_metrics_sort_without_removing_rows() {
    let mut models = vec![
        ModelUsage {
            model: "small".into(),
            input: 10,
            ..Default::default()
        },
        ModelUsage {
            model: "large".into(),
            input: 90,
            ..Default::default()
        },
    ];
    sort_model_usage(
        &mut models,
        SortState {
            column: Some(ModelSortColumn::Input),
            direction: SortDirection::Descending,
        },
    );
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].model, "large");
}

#[test]
fn every_model_metric_can_become_the_active_sort() {
    for column in [
        ModelSortColumn::Input,
        ModelSortColumn::CacheRead,
        ModelSortColumn::CacheWrite,
        ModelSortColumn::Output,
        ModelSortColumn::Reasoning,
        ModelSortColumn::Messages,
        ModelSortColumn::Turns,
        ModelSortColumn::Total,
        ModelSortColumn::Cost,
    ] {
        let mut state = ModelTablesState::new();
        if column == ModelSortColumn::Cost {
            state.toggle_sort(Page::Hourly, ModelSortColumn::Input);
        }
        state.toggle_sort(Page::Hourly, column);
        assert_eq!(state.sort(Page::Hourly).column, Some(column));
        assert_eq!(
            state.sort(Page::Hourly).direction,
            SortDirection::Descending
        );
        state.toggle_sort(Page::Hourly, column);
        assert_eq!(state.sort(Page::Hourly).direction, SortDirection::Ascending);
    }
}

#[test]
fn usage_tables_keep_all_nonzero_history_available_for_global_sorting() {
    let usage = UsageSeries {
        daily: vec![
            UsageBucket {
                key: "2024-01-01".into(),
                tokens: 1,
                cost: 900.0,
                ..Default::default()
            },
            UsageBucket {
                key: "2026-08-18".into(),
                tokens: 1,
                cost: 1.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut rows = visible_usage_buckets(&usage, UsagePeriod::Daily);
    assert_eq!(rows.len(), 2);
    sort_usage_buckets(
        &mut rows,
        SortState {
            column: Some(UsageSortColumn::Cost),
            direction: SortDirection::Descending,
        },
    );
    assert_eq!(rows[0].key, "2024-01-01");
}

#[test]
fn model_totals_merge_and_sort_by_tokens() {
    let models = [
        ModelUsage {
            model: "small".into(),
            provider: "openai".into(),
            tokens: 20,
            cost: 1.0,
            ..Default::default()
        },
        ModelUsage {
            model: "large".into(),
            provider: "anthropic".into(),
            tokens: 50,
            cost: 2.0,
            ..Default::default()
        },
        ModelUsage {
            model: "small".into(),
            provider: "openai".into(),
            tokens: 40,
            cost: 3.0,
            ..Default::default()
        },
    ];
    let totals = aggregate_model_usage(&models);
    assert_eq!(totals[0].model, "small");
    assert_eq!(totals[0].tokens, 60);
    assert_eq!(totals[0].cost, 4.0);
    assert_eq!(totals[1].model, "large");
}
