use chrono::{Datelike, Duration};
use gpui::{div, prelude::*, px, App, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use tokscope_core::history::{HistorySnapshot, UsagePeriod};

use crate::Page;

use super::{
    claude_accent, codex_accent, current_usage_date, provider_point, provider_usage_chart,
    section_title, source_bucket_values, summary_chart_row, usage_chart_points,
    usage_summary_sidebar, ProviderPoint, UsageSummary,
};

pub(super) fn usage_block(h: &HistorySnapshot, cx: &App) -> gpui::Div {
    h_flex()
        .w_full()
        .flex_wrap()
        .gap_4()
        .child(overview_range_card(
            "Today",
            "Usage by hour",
            usage_chart_points(h, UsagePeriod::Daily),
            "overview-today",
            super::page_accent(Page::Daily, cx),
            cx,
        ))
        .child(overview_range_card(
            "This month",
            "Usage by day",
            overview_usage_points(h),
            "overview-month",
            super::page_accent(Page::Monthly, cx),
            cx,
        ))
}

fn overview_range_card(
    title: &'static str,
    cadence: &'static str,
    data: Vec<ProviderPoint>,
    id_prefix: &'static str,
    accent: Hsla,
    cx: &App,
) -> gpui::Div {
    let summary = usage_summary_sidebar(UsageSummary::from_points(&data), "EST. API COST", cx);

    v_flex()
        .debug_selector(move || format!("{id_prefix}-card"))
        .flex_1()
        .min_w(px(700.))
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
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().size_2().rounded_full().bg(accent))
                        .child(section_title(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(cadence),
                        ),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .child(legend_chip("Codex", codex_accent(), cx))
                        .child(legend_chip("Claude Code", claude_accent(), cx)),
                ),
        )
        .child(summary_chart_row(
            summary,
            provider_usage_chart(data, id_prefix, cx),
        ))
}

/// Month-to-date points used by the secondary Overview chart.
pub(super) fn overview_usage_points(h: &HistorySnapshot) -> Vec<ProviderPoint> {
    let claude = h.source("claude");
    let codex = h.source("codex");
    let today = current_usage_date(h);
    let first = today.with_day(1).unwrap_or(today);
    (0..i64::from(today.day()))
        .map(|offset| {
            let date = first + Duration::days(offset);
            let key = date.format("%Y-%m-%d").to_string();
            provider_point(
                date.format("%A, %B %-d, %Y").to_string(),
                date.format("%m-%d").to_string(),
                source_bucket_values(claude, |usage| &usage.daily, &key),
                source_bucket_values(codex, |usage| &usage.daily, &key),
            )
        })
        .collect()
}

pub(super) fn legend_chip(label: &'static str, color: Hsla, cx: &App) -> gpui::Div {
    h_flex()
        .gap_1p5()
        .items_center()
        .child(div().size_2().rounded_full().bg(color))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
}
