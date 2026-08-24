use super::{sort_action, table_cell, table_sort_header, TableColumn, TableContext, UsageColumn};
use crate::{Page, UsageSortColumn};
use chrono::NaiveDate;
use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use toks_core::history::UsagePeriod;
pub(super) fn usage_columns_header(
    period: UsagePeriod,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> gpui::Div {
    let cx = table.cx();
    let period_label = match period {
        UsagePeriod::Hourly => "Time",
        UsagePeriod::Daily | UsagePeriod::Monthly => "Period",
    };
    let mut header = h_flex()
        .gap_2()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(
            div()
                .flex_1()
                .min_w(px(UsageColumn::LABEL_WIDTH))
                .child(usage_period_sort_header(period_label, period, table).justify_start()),
        );
    for column in table.columns::<UsageColumn>() {
        header = header.child(usage_sort_header(column, period, table));
    }
    header
}

pub(super) fn usage_static_columns_header(
    first: &'static str,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> gpui::Div {
    let cx = table.cx();
    let mut header = h_flex()
        .gap_2()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(
            div()
                .flex_1()
                .min_w(px(UsageColumn::LABEL_WIDTH))
                .child(first),
        );
    for column in table.columns::<UsageColumn>() {
        header = header.child(static_header(column));
    }
    header
}

pub(super) fn hourly_day_separator(date: NaiveDate, cx: &App) -> gpui::Div {
    div()
        .debug_selector(move || format!("usage-day-{date}"))
        .pt_3()
        .pb_1()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(date.format("%A, %B %-d, %Y").to_string())
}

fn usage_period_sort_header(
    label: &'static str,
    period: UsagePeriod,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> Button {
    let column = UsageSortColumn::Period;
    let sort = table.sort();
    sort_action(
        SharedString::from(format!("usage-sort-{}-period", Page::from(period).slug())),
        label,
        UsageColumn::LABEL_WIDTH,
        sort.column == Some(column),
        sort.direction,
        table.cx(),
    )
    .on_click(table.cx().listener(move |app, _, _, cx| {
        app.usage_tables.toggle_sort(period, column);
        cx.notify();
    }))
}

fn usage_sort_header(
    column: UsageColumn,
    period: UsagePeriod,
    table: TableContext<'_, '_, UsageSortColumn>,
) -> Button {
    let sort_column = column.sort_column();
    table_sort_header(
        SharedString::from(format!(
            "usage-sort-{}-{}",
            Page::from(period).slug(),
            column.id()
        )),
        column,
        table.sort(),
        table.cx(),
    )
    .on_click(table.cx().listener(move |app, _, _, cx| {
        app.usage_tables.toggle_sort(period, sort_column);
        cx.notify();
    }))
}

fn static_header(column: UsageColumn) -> gpui::Div {
    table_cell(column)
        .debug_selector(move || format!("usage-static-header-{}", column.id()))
        .child(column.label())
}
