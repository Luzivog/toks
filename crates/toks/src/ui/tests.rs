use super::{
    fmt_age, fmt_cost_per_million, hourly_bucket_day, sort_model_usage, sort_usage_buckets,
    split_limit_label, usage_bucket_is_current, usage_bucket_label, usage_hover_geometry,
    usage_marker_top, visible_usage_buckets, visible_usage_row_count,
};
use crate::{ModelSortColumn, ModelTablesState, Page, SortDirection, SortState, UsageSortColumn};
use chrono::{NaiveDate, TimeZone, Utc};
use toks_core::history::{ModelUsage, UsageBucket, UsagePeriod, UsageSeries};

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
fn aggregate_cost_per_million_is_clear_and_zero_safe() {
    assert_eq!(fmt_cost_per_million(2.5, 5_000_000), "$0.50");
    assert_eq!(fmt_cost_per_million(0.0, 0), "—");
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
        ["2026-08-18 09:00", "2026-08-18 01:00", "2026-08-17 23:00"]
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
        ["2026-08-17 23:00", "2026-08-18 01:00", "2026-08-18 09:00"]
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
    let totals = super::aggregate_model_usage(&models);
    assert_eq!(totals[0].model, "small");
    assert_eq!(totals[0].tokens, 60);
    assert_eq!(totals[0].cost, 4.0);
    assert_eq!(totals[1].model, "large");
}
