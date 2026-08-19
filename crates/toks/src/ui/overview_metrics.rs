use chrono::Datelike;
use gpui::{prelude::*, App};
use gpui_component::v_flex;
use toks_core::history::{HistorySnapshot, UsageBucket};

use super::{current_usage_date, usage_metric_row, usage_static_columns_header};

pub(super) fn overview_metrics_card(history: &HistorySnapshot, cx: &App) -> gpui::Div {
    let today = current_usage_date(history);
    let today_key = today.format("%Y-%m-%d").to_string();
    let month_key = format!("{}-{:02}", today.year(), today.month());
    let today_bucket = bucket_or_empty(&history.usage.daily, &today_key);
    let month_bucket = bucket_or_empty(&history.usage.monthly, &month_key);

    v_flex()
        .debug_selector(|| "overview-current-usage".to_string())
        .mt_5()
        .pt_3()
        .child(usage_static_columns_header("Range", cx))
        .child(usage_metric_row(
            "overview-usage-today".into(),
            "Today".into(),
            &today_bucket,
            false,
            cx,
        ))
        .child(usage_metric_row(
            "overview-usage-month".into(),
            "Month to date".into(),
            &month_bucket,
            false,
            cx,
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
