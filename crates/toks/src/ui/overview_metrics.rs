use chrono::Datelike;
use gpui::prelude::*;
use gpui_component::v_flex;
use toks_core::history::{HistorySnapshot, UsageBucket};

use crate::UsageSortColumn;

use super::{current_usage_date, usage_metric_row, usage_static_columns_header, TableContext};

pub(super) fn overview_metrics_card(
    history: &HistorySnapshot,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> gpui::Div {
    let today = current_usage_date(history);
    let today_key = today.format("%Y-%m-%d").to_string();
    let month_key = format!("{}-{:02}", today.year(), today.month());
    let today_bucket = bucket_or_empty(&history.usage.daily, &today_key);
    let month_bucket = bucket_or_empty(&history.usage.monthly, &month_key);

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
