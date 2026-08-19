use chrono::Duration;
use gpui::{div, prelude::*, App, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use tokscope_core::history::HistorySnapshot;

use super::{
    claude_accent, codex_accent, current_usage_date, overview_metrics_card, provider_point,
    provider_usage_chart, section_title, source_bucket_values, summary_chart_row,
    usage_summary_sidebar, ProviderPoint, UsageSummary,
};

pub(super) fn usage_block(
    history: &HistorySnapshot,
    refresh_label: Option<String>,
    cx: &App,
) -> gpui::Div {
    last_thirty_days_card(history, refresh_label, cx)
}

fn last_thirty_days_card(
    history: &HistorySnapshot,
    refresh_label: Option<String>,
    cx: &App,
) -> gpui::Div {
    let data = overview_usage_points(history);
    let summary = usage_summary_sidebar(UsageSummary::from_points(&data), "EST. API COST", cx);
    v_flex()
        .debug_selector(|| "overview-usage-card".to_string())
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
                        .items_center()
                        .gap_2()
                        .child(section_title("Usage — last 30 days"))
                        .when_some(refresh_label, |heading, label| {
                            heading.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                        }),
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
            provider_usage_chart(data, "overview-usage", cx),
        ))
        .child(overview_metrics_card(history, cx))
}

pub(super) fn overview_usage_points(history: &HistorySnapshot) -> Vec<ProviderPoint> {
    let claude = history.source("claude");
    let codex = history.source("codex");
    let today = current_usage_date(history);
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
