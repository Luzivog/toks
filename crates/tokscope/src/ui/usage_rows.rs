use super::{
    fmt_cost_full, fmt_tokens, hourly_bucket_full_label, page_accent, sort_action,
    usage_bucket_label,
};
use crate::{Page, SortState, TokscopeApp, UsageSortColumn};
use chrono::NaiveDate;
use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use tokscope_core::history::{UsageBucket, UsagePeriod};
pub(super) fn usage_columns_header(
    period: UsagePeriod,
    sort: SortState<UsageSortColumn>,
    cx: &mut gpui::Context<TokscopeApp>,
) -> gpui::Div {
    let period_label = match period {
        UsagePeriod::Hourly => "Time",
        UsagePeriod::Daily | UsagePeriod::Monthly => "Period",
    };
    h_flex()
        .gap_3()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(
            div().flex_1().min_w(px(130.)).child(
                usage_sort_header(
                    period_label,
                    130.,
                    UsageSortColumn::Period,
                    period,
                    sort,
                    cx,
                )
                .justify_start(),
            ),
        )
        .child(usage_sort_header(
            "Turns",
            58.,
            UsageSortColumn::Turns,
            period,
            sort,
            cx,
        ))
        .child(usage_sort_header(
            "Messages",
            72.,
            UsageSortColumn::Messages,
            period,
            sort,
            cx,
        ))
        .child(usage_sort_header(
            "Input",
            82.,
            UsageSortColumn::Input,
            period,
            sort,
            cx,
        ))
        .child(usage_sort_header(
            "Output",
            82.,
            UsageSortColumn::Output,
            period,
            sort,
            cx,
        ))
        .child(usage_sort_header(
            "Cache read",
            88.,
            UsageSortColumn::CacheRead,
            period,
            sort,
            cx,
        ))
        .child(usage_sort_header(
            "Total",
            88.,
            UsageSortColumn::Total,
            period,
            sort,
            cx,
        ))
        .child(usage_sort_header(
            "Est. API cost",
            98.,
            UsageSortColumn::Cost,
            period,
            sort,
            cx,
        ))
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

pub(super) fn usage_data_row(
    bucket: &UsageBucket,
    period: UsagePeriod,
    grouped_hourly: bool,
    highlighted: bool,
    cx: &App,
) -> gpui::Div {
    let selector = format!("usage-row-{}-{}", usage_period_id(period), bucket.key);
    let label = if period == UsagePeriod::Hourly && !grouped_hourly {
        hourly_bucket_full_label(&bucket.key)
    } else {
        usage_bucket_label(period, &bucket.key)
    };
    h_flex()
        .debug_selector(move || selector)
        .gap_3()
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
        .child(div().flex_1().min_w(px(130.)).font_medium().child(label))
        .child(metric_cell(58., fmt_tokens(bucket.turns), false))
        .child(metric_cell(72., fmt_tokens(bucket.messages), false))
        .child(metric_cell(82., fmt_tokens(bucket.input), false))
        .child(metric_cell(82., fmt_tokens(bucket.output), false))
        .child(metric_cell(88., fmt_tokens(bucket.cache_read), false))
        .child(metric_cell(88., fmt_tokens(bucket.tokens), true))
        .child(metric_cell(98., fmt_cost_full(bucket.cost), true))
}

fn usage_sort_header(
    label: &'static str,
    width: f32,
    column: UsageSortColumn,
    period: UsagePeriod,
    sort: SortState<UsageSortColumn>,
    cx: &mut gpui::Context<TokscopeApp>,
) -> Button {
    let active = sort.column == Some(column);
    sort_action(
        SharedString::from(format!(
            "usage-sort-{}-{}",
            usage_period_id(period),
            usage_sort_column_id(column)
        )),
        label,
        width,
        active,
        sort.direction,
        cx,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        app.usage_tables.toggle_sort(period, column);
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

fn usage_sort_column_id(column: UsageSortColumn) -> &'static str {
    match column {
        UsageSortColumn::Period => "period",
        UsageSortColumn::Turns => "turns",
        UsageSortColumn::Messages => "messages",
        UsageSortColumn::Input => "input",
        UsageSortColumn::Output => "output",
        UsageSortColumn::CacheRead => "cache-read",
        UsageSortColumn::Total => "total",
        UsageSortColumn::Cost => "cost",
    }
}

fn metric_cell(width: f32, value: String, emphasized: bool) -> gpui::Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_right()
        .when(emphasized, |cell| cell.font_semibold())
        .child(value)
}
