use gpui::{div, prelude::*, px, Hsla};
use gpui_component::{h_flex, ActiveTheme, StyledExt};
use toks_core::history::{UsageBucket, UsagePeriod};

use super::{
    hourly_bucket_full_label, page_accent, table_cell, usage_bucket_label, TableColumn,
    TableContext, UsageColumn,
};
use crate::{Page, UsageSortColumn};

pub(super) fn usage_data_row(
    bucket: &UsageBucket,
    period: UsagePeriod,
    grouped_hourly: bool,
    highlighted: bool,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> gpui::Div {
    let cx = table.cx();
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
        highlighted.then(|| page_accent(Page::from(period), cx)),
        table,
    )
}

pub(super) fn usage_metric_row(
    selector: String,
    label: String,
    bucket: &UsageBucket,
    highlight_color: Option<Hsla>,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> gpui::Div {
    let cx = table.cx();
    let row_selector = selector.clone();
    let mut row = h_flex()
        .debug_selector(move || row_selector.clone())
        .gap_2()
        .min_w_0()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(highlight_color.unwrap_or(cx.theme().foreground))
        .child(
            div()
                .flex_1()
                .min_w(px(UsageColumn::LABEL_WIDTH))
                .font_medium()
                .child(label),
        );
    for column in table.columns::<UsageColumn>() {
        row = row.child(metric_cell(
            format!("{selector}-{}", column.id()),
            column,
            column.value(bucket),
        ));
    }
    row
}

fn metric_cell(selector: String, column: UsageColumn, value: String) -> gpui::Div {
    table_cell(column)
        .debug_selector(move || selector.clone())
        .when(column.emphasized(), |cell| cell.font_semibold())
        .child(value)
}

fn period_id(period: UsagePeriod) -> &'static str {
    Page::from(period).slug()
}
