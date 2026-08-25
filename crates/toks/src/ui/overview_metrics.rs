use chrono::Datelike;
use gpui::prelude::*;
use gpui_component::v_flex;
use toks_core::history::{HistorySnapshot, UsageBucket, UsageKey, UsagePeriod, UsageSeries};

use crate::UsageSortColumn;

use super::{
    current_usage_date, usage_metric_row, usage_static_columns_header, week_start, TableContext,
};

pub(super) fn overview_metrics_card(
    history: &HistorySnapshot,
    usage: &UsageSeries,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> gpui::Div {
    let today = current_usage_date(history);
    let today_key = today.format("%Y-%m-%d").to_string();
    let month_key = format!("{}-{:02}", today.year(), today.month());
    let today_bucket = bucket_or_empty(&usage.daily, &today_key);
    let week_bucket = week_to_date_bucket(usage, today);
    let month_bucket = bucket_or_empty(&usage.monthly, &month_key);

    v_flex()
        .debug_selector(|| "overview-current-usage".to_string())
        .mt_5()
        .pt_3()
        .child(usage_static_columns_header("Range", table))
        .child(usage_metric_row(
            "overview-usage-today".into(),
            "Today".into(),
            &today_bucket,
            None,
            table,
        ))
        .child(usage_metric_row(
            "overview-usage-week".into(),
            "This week".into(),
            &week_bucket,
            None,
            table,
        ))
        .child(usage_metric_row(
            "overview-usage-month".into(),
            "Month to date".into(),
            &month_bucket,
            None,
            table,
        ))
}

fn bucket_or_empty(buckets: &[UsageBucket], key: &str) -> UsageBucket {
    buckets
        .iter()
        .find(|bucket| bucket.key == key)
        .cloned()
        .unwrap_or_else(|| UsageBucket {
            key: key.into(),
            ..Default::default()
        })
}

pub(super) fn week_to_date_bucket(usage: &UsageSeries, today: chrono::NaiveDate) -> UsageBucket {
    let start = week_start(today);
    let mut total = UsageBucket {
        key: start.format("%Y-%m-%d").to_string(),
        ..Default::default()
    };
    for bucket in &usage.daily {
        let Some(UsageKey::Daily(date)) = bucket.typed_key(UsagePeriod::Daily) else {
            continue;
        };
        if (start..=today).contains(&date) {
            add_usage(&mut total, bucket);
        }
    }
    total
}

fn add_usage(total: &mut UsageBucket, bucket: &UsageBucket) {
    total.input = total.input.saturating_add(bucket.input);
    total.output = total.output.saturating_add(bucket.output);
    total.cache_read = total.cache_read.saturating_add(bucket.cache_read);
    total.cache_write = total.cache_write.saturating_add(bucket.cache_write);
    total.reasoning = total.reasoning.saturating_add(bucket.reasoning);
    total.tokens = total.tokens.saturating_add(bucket.tokens);
    total.messages = total.messages.saturating_add(bucket.messages);
    total.turns = total.turns.saturating_add(bucket.turns);
    total.cost += bucket.cost;
    total.cost_coverage.covered_tokens = total
        .cost_coverage
        .covered_tokens
        .saturating_add(bucket.cost_coverage.covered_tokens);
    total.cost_coverage.uncovered_tokens = total
        .cost_coverage
        .uncovered_tokens
        .saturating_add(bucket.cost_coverage.uncovered_tokens);
    total.cost_coverage.covered_messages = total
        .cost_coverage
        .covered_messages
        .saturating_add(bucket.cost_coverage.covered_messages);
    total.cost_coverage.uncovered_messages = total
        .cost_coverage
        .uncovered_messages
        .saturating_add(bucket.cost_coverage.uncovered_messages);
    total.cost_coverage.invalid_records = total
        .cost_coverage
        .invalid_records
        .saturating_add(bucket.cost_coverage.invalid_records);
}
