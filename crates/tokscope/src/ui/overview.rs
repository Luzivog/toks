use chrono::Duration;
use gpui::{div, prelude::*, App, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use tokscope_core::history::HistorySnapshot;

use super::{
    claude_accent, codex_accent, current_usage_date, provider_point, provider_usage_chart,
    section_title, source_bucket_values, usage_summary_sidebar, ProviderPoint,
};

pub(super) fn usage_block(h: &HistorySnapshot, cx: &App) -> gpui::Div {
    let data = overview_usage_points(h);
    let summary = usage_summary_sidebar(&data, "EST. API COST", cx);

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
                .child(section_title("Cost — last 30 days"))
                .child(
                    h_flex()
                        .gap_3()
                        .child(legend_chip("Codex", codex_accent(), cx))
                        .child(legend_chip("Claude Code", claude_accent(), cx)),
                ),
        )
        .child(
            h_flex().gap_6().items_start().child(summary).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(provider_usage_chart(data, "overview-usage", cx)),
            ),
        )
}

pub(super) fn overview_usage_points(h: &HistorySnapshot) -> Vec<ProviderPoint> {
    let claude = h.source("claude");
    let codex = h.source("codex");
    let today = current_usage_date(h);
    (0..30)
        .map(|offset| {
            let date = today - Duration::days(29 - offset);
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
