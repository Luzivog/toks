use gpui::{div, prelude::*, px, App, Corner, Hsla};
use gpui_component::{
    h_flex,
    menu::{DropdownMenu, PopupMenuItem},
    v_flex, ActiveTheme,
};
use toks_core::history::HistorySnapshot;
use toks_core::{ProviderVisibility, USAGE_PROVIDERS};

use crate::{app::OverviewChartRange, ToksApp, UsageSortColumn};

use super::{
    accent_for_usage_provider, action_button, all_time_data::all_time_summary,
    overview_metrics_card, overview_usage_points, provider_usage_chart, section_title,
    summary_chart_row, usage_provider_label, usage_summary_sidebar, visible_usage, TableContext,
    TableLayout, UsageSummary,
};

pub(super) fn usage_block(
    history: &HistorySnapshot,
    refresh_label: Option<String>,
    layout: TableLayout,
    range: OverviewChartRange,
    visibility: &ProviderVisibility,
    cx: &gpui::Context<'_, ToksApp>,
) -> gpui::Div {
    overview_usage_card(history, refresh_label, layout, range, visibility, cx)
}

fn overview_usage_card(
    history: &HistorySnapshot,
    refresh_label: Option<String>,
    layout: TableLayout,
    range: OverviewChartRange,
    visibility: &ProviderVisibility,
    cx: &gpui::Context<'_, ToksApp>,
) -> gpui::Div {
    let data = overview_usage_points(history, range, visibility);
    let usage = visible_usage(history, visibility);
    let totals = if range == OverviewChartRange::AllTime {
        all_time_summary(history, visibility)
    } else {
        UsageSummary::from_points(&data)
    };
    let summary = usage_summary_sidebar(totals, visibility, "EST. API COST", cx);
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
                        .child(
                            div()
                                .debug_selector(move || {
                                    format!("overview-usage-title-{}", range.slug())
                                })
                                .child(section_title(range.title())),
                        )
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
                        .items_center()
                        .gap_3()
                        .child(usage_legend(visibility, cx))
                        .child(range_selector(range, cx)),
                ),
        )
        .child(summary_chart_row(
            summary,
            provider_usage_chart(data, range.chart_id(), visibility, cx),
        ))
        .child(overview_metrics_card(
            history,
            &usage,
            TableContext::<UsageSortColumn>::unsorted(layout, cx),
        ))
}

fn range_selector(range: OverviewChartRange, cx: &gpui::Context<'_, ToksApp>) -> impl IntoElement {
    let app = cx.entity().downgrade();
    action_button("overview-range-selector", cx)
        .compact()
        .w(px(112.))
        .h(px(26.))
        .justify_end()
        .text_xs()
        .dropdown_caret(true)
        .label(range.label())
        .dropdown_menu_with_anchor(Corner::TopRight, move |mut popup, _, _| {
            for choice in OverviewChartRange::ALL {
                popup = popup.item(range_menu_item(choice, choice == range, app.clone()));
            }
            popup
        })
}

fn range_menu_item(
    range: OverviewChartRange,
    checked: bool,
    app: gpui::WeakEntity<ToksApp>,
) -> PopupMenuItem {
    let selector = format!("overview-range-{}", range.slug());
    PopupMenuItem::element(move |_, _| {
        let selector = selector.clone();
        div()
            .debug_selector(move || selector.clone())
            .size_full()
            .flex()
            .items_center()
            .child(range.label())
    })
    .checked(checked)
    .on_click(move |_, _, cx| {
        let _ = app.update(cx, |app, cx| {
            app.set_overview_chart_range(range);
            cx.notify();
        });
    })
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
