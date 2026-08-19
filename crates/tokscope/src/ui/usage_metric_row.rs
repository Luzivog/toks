use gpui::{div, prelude::*, px, App};
use gpui_component::{h_flex, ActiveTheme, StyledExt};
use tokscope_core::history::{UsageBucket, UsagePeriod};

use super::{hourly_bucket_full_label, page_accent, usage_bucket_label, UsageColumn};
use crate::Page;

pub(super) fn usage_data_row(
    bucket: &UsageBucket,
    period: UsagePeriod,
    grouped_hourly: bool,
    highlighted: bool,
    cx: &App,
) -> gpui::Div {
    let selector = format!("usage-row-{}-{}", period_id(period), bucket.key);
    let label = if period == UsagePeriod::Hourly && !grouped_hourly {
        hourly_bucket_full_label(&bucket.key)
    } else {
        usage_bucket_label(period, &bucket.key)
    };
    usage_metric_row(selector, label, bucket, highlighted, cx)
}

pub(super) fn usage_metric_row(
    selector: String,
    label: String,
    bucket: &UsageBucket,
    highlighted: bool,
    cx: &App,
) -> gpui::Div {
    let mut row = h_flex()
        .debug_selector(move || selector)
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
    for column in UsageColumn::ALL {
        row = row.child(metric_cell(
            column.width(),
            column.value(bucket),
            column.emphasized(),
        ));
    }
    row
}

fn metric_cell(width: f32, value: String, emphasized: bool) -> gpui::Div {
    div()
        .w(px(width))
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
