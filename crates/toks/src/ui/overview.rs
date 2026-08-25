use chrono::Duration;
use gpui::{div, prelude::*, App, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::history::HistorySnapshot;
use toks_core::{ClientId, ProviderVisibility, USAGE_PROVIDERS};

use crate::{ToksApp, UsageSortColumn};

use super::{
    accent_for_usage_provider, current_usage_date, overview_metrics_card, provider_point,
    provider_usage_chart, section_title, source_bucket_values, summary_chart_row,
    usage_provider_label, usage_summary_sidebar, visible_source, visible_usage, ProviderPoint,
    TableContext, TableLayout, UsageSummary,
};

pub(super) fn usage_block(
    history: &HistorySnapshot,
    refresh_label: Option<String>,
    layout: TableLayout,
    visibility: &ProviderVisibility,
    cx: &gpui::Context<'_, ToksApp>,
) -> gpui::Div {
    last_thirty_days_card(history, refresh_label, layout, visibility, cx)
}

fn last_thirty_days_card(
    history: &HistorySnapshot,
    refresh_label: Option<String>,
    layout: TableLayout,
    visibility: &ProviderVisibility,
    cx: &gpui::Context<'_, ToksApp>,
) -> gpui::Div {
    let data = overview_usage_points(history, visibility);
    let usage = visible_usage(history, visibility);
    let summary = usage_summary_sidebar(
        UsageSummary::from_points(&data),
        visibility,
        "EST. API COST",
        cx,
    );
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
                .child(usage_legend(visibility, cx)),
        )
        .child(summary_chart_row(
            summary,
            provider_usage_chart(data, "overview-usage", visibility, cx),
        ))
        .child(overview_metrics_card(
            history,
            &usage,
            TableContext::<UsageSortColumn>::unsorted(layout, cx),
        ))
}

pub(super) fn overview_usage_points(
    history: &HistorySnapshot,
    visibility: &ProviderVisibility,
) -> Vec<ProviderPoint> {
    let claude = visible_source(history, ClientId::Claude, visibility);
    let codex = visible_source(history, ClientId::Codex, visibility);
    let opencode = visible_source(history, ClientId::OpenCode, visibility);
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
                source_bucket_values(opencode, |usage| &usage.daily, &key),
            )
        })
        .collect()
}

pub(super) fn usage_legend(visibility: &ProviderVisibility, cx: &App) -> gpui::Div {
    let mut legend = h_flex().gap_3();
    for provider in USAGE_PROVIDERS {
        if visibility.is_visible(provider) {
            legend = legend.child(
                legend_chip(
                    usage_provider_label(provider),
                    accent_for_usage_provider(provider),
                    cx,
                )
                .debug_selector(move || format!("usage-legend-{}", provider.as_str())),
            );
        }
    }
    legend
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
