use super::{sort_action, TableLayout, UsageColumn};
use crate::{SortState, ToksApp, UsageSortColumn};
use chrono::NaiveDate;
use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use toks_core::history::UsagePeriod;
pub(super) fn usage_columns_header(
    period: UsagePeriod,
    sort: SortState<UsageSortColumn>,
    layout: TableLayout,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
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
                .min_w(px(130.))
                .child(usage_period_sort_header(period_label, period, sort, cx).justify_start()),
        );
    for column in layout.usage_columns(sort.column) {
        header = header.child(usage_sort_header(column, period, sort, cx));
    }
    header
}

pub(super) fn usage_static_columns_header(
    first: &'static str,
    layout: TableLayout,
    cx: &App,
) -> gpui::Div {
    let mut header = h_flex()
        .gap_2()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(div().flex_1().min_w(px(130.)).child(first));
    for column in layout.usage_columns(None) {
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
    sort: SortState<UsageSortColumn>,
    cx: &mut gpui::Context<ToksApp>,
) -> Button {
    let column = UsageSortColumn::Period;
    sort_action(
        SharedString::from(format!("usage-sort-{}-period", usage_period_id(period))),
        label,
        130.,
        sort.column == Some(column),
        sort.direction,
        cx,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        app.usage_tables.toggle_sort(period, column);
        cx.notify();
    }))
}

fn usage_sort_header(
    column: UsageColumn,
    period: UsagePeriod,
    sort: SortState<UsageSortColumn>,
    cx: &mut gpui::Context<ToksApp>,
) -> Button {
    let sort_column = column.sort_column();
    let active = sort.column == Some(sort_column);
    sort_action(
        SharedString::from(format!(
            "usage-sort-{}-{}",
            usage_period_id(period),
            column.id()
        )),
        column.label(),
        column.width(),
        active,
        sort.direction,
        cx,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        app.usage_tables.toggle_sort(period, sort_column);
        cx.notify();
    }))
}

fn usage_period_id(period: UsagePeriod) -> &'static str {
    match period {
        UsagePeriod::Hourly => "hourly",
        UsagePeriod::Daily => "daily",
        UsagePeriod::Monthly => "monthly",
    }
}

fn static_header(column: UsageColumn) -> gpui::Div {
    div()
        .debug_selector(move || format!("usage-static-header-{}", column.id()))
        .w(px(column.width()))
        .flex_shrink_0()
        .text_right()
        .child(column.label())
}
