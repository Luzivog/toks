use gpui::{div, prelude::*, App, Hsla};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::history::{HistorySnapshot, UsagePeriod};
use toks_core::ProviderVisibility;

use super::{
    provider_usage_chart, section_title, summary_chart_row, usage_chart_points, usage_legend,
    usage_summary_sidebar, UsageSummary,
};

pub(super) fn usage_chart_card(
    history: &HistorySnapshot,
    period: UsagePeriod,
    accent: Hsla,
    visibility: &ProviderVisibility,
    cx: &App,
) -> gpui::Div {
    let (title, id_prefix) = usage_chart_identity(period);
    let data = usage_chart_points(history, period, visibility);
    let summary = usage_summary_sidebar(
        UsageSummary::from_points(&data),
        visibility,
        "EST. API COST",
        cx,
    );

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
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().size_2().rounded_full().bg(accent))
                        .child(section_title(title)),
                )
                .child(usage_legend(visibility, cx)),
        )
        .child(summary_chart_row(
            summary,
            provider_usage_chart(data, id_prefix, visibility, cx),
        ))
}

pub(super) fn usage_chart_identity(period: UsagePeriod) -> (&'static str, &'static str) {
    match period {
        UsagePeriod::Hourly => ("Usage — last 60 minutes", "hourly-usage"),
        UsagePeriod::Daily => ("Usage — today by hour", "daily-usage"),
        UsagePeriod::Monthly => ("Usage — this month by day", "monthly-usage"),
    }
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------
