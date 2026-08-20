use chrono::{Local, TimeZone};
use gpui::{div, prelude::*};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::history::{HistorySnapshot, UsagePeriod};

use crate::{SortState, ToksApp, UsageSortColumn};

use super::{
    current_usage_date, hourly_bucket_day, hourly_day_separator, section_meta, section_title,
    sort_usage_buckets, text_action, usage_bucket_is_current, usage_columns_header, usage_data_row,
    usage_period_label, usage_range_label, visible_usage_buckets, TableLayout,
};

pub(super) fn visible_usage_row_count(total: usize, visible_limit: usize) -> usize {
    total.min(visible_limit)
}

pub(super) fn usage_history_card(
    history: &HistorySnapshot,
    period: UsagePeriod,
    sort: SortState<UsageSortColumn>,
    visible_limit: usize,
    freshness: Option<String>,
    layout: TableLayout,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let generated = Local
        .timestamp_millis_opt(history.generated_at_ms)
        .single()
        .unwrap_or_else(Local::now);
    let current_date = current_usage_date(history);
    let latest = match period {
        UsagePeriod::Hourly => format!(
            "{} {}:00",
            current_date.format("%Y-%m-%d"),
            generated.format("%H")
        ),
        UsagePeriod::Daily => current_date.format("%Y-%m-%d").to_string(),
        UsagePeriod::Monthly => current_date.format("%Y-%m").to_string(),
    };
    let mut rows = visible_usage_buckets(&history.usage, period);
    sort_usage_buckets(&mut rows, sort);
    let row_count = rows.len();
    let visible_count = visible_usage_row_count(row_count, visible_limit);
    let grouped_hourly = period == UsagePeriod::Hourly
        && matches!(sort.column, None | Some(UsageSortColumn::Period));
    let range = format!("{} · {row_count} rows", usage_range_label());
    let context = freshness
        .map(|freshness| format!("{freshness} · {range}"))
        .unwrap_or(range);

    let mut body = v_flex();
    if rows.is_empty() {
        body = body.child(
            div()
                .py_8()
                .text_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "No {} usage recorded yet.",
                    usage_period_label(period).to_lowercase()
                )),
        );
    } else {
        let mut previous_day = None;
        for bucket in rows.into_iter().take(visible_count) {
            if grouped_hourly {
                if let Some(day) = hourly_bucket_day(&bucket.key) {
                    if previous_day != Some(day) {
                        body = body.child(hourly_day_separator(day, cx));
                        previous_day = Some(day);
                    }
                }
            }
            let highlighted = usage_bucket_is_current(period, &bucket.key, &latest);
            body = body.child(usage_data_row(
                bucket,
                period,
                grouped_hourly,
                highlighted,
                layout,
                sort.column,
                cx,
            ));
        }
    }

    v_flex()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .gap_3()
                .child(section_title(format!(
                    "{} usage",
                    usage_period_label(period)
                )))
                .child(section_meta(context, cx)),
        )
        .child(usage_columns_header(period, sort, layout, cx))
        .child(body)
        .when(visible_count < row_count, |card| {
            let next = (row_count - visible_count).min(crate::USAGE_PAGE_SIZE);
            let label = format!("Show {next} more");
            card.child(
                h_flex()
                    .justify_center()
                    // The card already contributes 16px below this row. A
                    // 12px inset above keeps the action optically centered
                    // between the table divider and the card edge.
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        text_action(
                            match period {
                                UsagePeriod::Hourly => "hourly-usage-more",
                                UsagePeriod::Daily => "daily-usage-more",
                                UsagePeriod::Monthly => "monthly-usage-more",
                            },
                            label,
                            cx,
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.usage_tables.show_more(period);
                            cx.notify();
                        })),
                    ),
            )
        })
}
