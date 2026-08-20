use gpui::{div, prelude::*, px, App};
use gpui_component::{h_flex, ActiveTheme, StyledExt};
use toks_core::history::{UsageBucket, UsagePeriod};

use super::{hourly_bucket_full_label, page_accent, usage_bucket_label, TableLayout, UsageColumn};
use crate::{Page, UsageSortColumn};

pub(super) fn usage_data_row(
    bucket: &UsageBucket,
    period: UsagePeriod,
    grouped_hourly: bool,
    highlighted: bool,
    layout: TableLayout,
    active_sort: Option<UsageSortColumn>,
    cx: &App,
) -> gpui::Div {
    let selector = format!("usage-row-{}-{}", period_id(period), bucket.key);
    let label = if period == UsagePeriod::Hourly && !grouped_hourly {
        hourly_bucket_full_label(&bucket.key)
    } else {
        usage_bucket_label(period, &bucket.key)
    };
    usage_metric_row(
        selector,
        label,
        bucket,
        highlighted,
        layout,
        active_sort,
        cx,
    )
}

pub(super) fn usage_metric_row(
    selector: String,
    label: String,
    bucket: &UsageBucket,
    highlighted: bool,
    layout: TableLayout,
    active_sort: Option<UsageSortColumn>,
    cx: &App,
) -> gpui::Div {
    let row_selector = selector.clone();
    let mut row = h_flex()
        .debug_selector(move || row_selector.clone())
        .gap_2()
        .min_w_0()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(if highlighted {
            page_accent(Page::Daily, cx)
        } else {
            cx.theme().foreground
        })
        .child(div().flex_1().min_w(px(130.)).font_medium().child(label));
    for column in layout.usage_columns(active_sort) {
        row = row.child(metric_cell(
            format!("{selector}-{}", column.id()),
            column,
            column.value(bucket),
            column.emphasized(),
        ));
    }
    row
}

fn metric_cell(
    selector: String,
    column: UsageColumn,
    value: String,
    emphasized: bool,
) -> gpui::Div {
    div()
        .debug_selector(move || selector.clone())
        .w(px(column.width()))
        .flex_shrink_0()
        .text_right()
        .when(emphasized, |cell| cell.font_semibold())
        .child(value)
}

fn period_id(period: UsagePeriod) -> &'static str {
    match period {
        UsagePeriod::Hourly => "hourly",
        UsagePeriod::Daily => "daily",
        UsagePeriod::Monthly => "monthly",
    }
}
