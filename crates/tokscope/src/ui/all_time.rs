use chrono::{Datelike, NaiveDate};
use gpui::{div, prelude::*, App};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use tokscope_core::history::HistorySnapshot;

use crate::{Page, TokscopeApp};

use super::{
    chart_plot::provider_usage_chart,
    chart_tooltip::ProviderPoint,
    loading_chart::{loading_plot, loading_status, loading_summary_sidebar},
    model_data::aggregate_model_usage,
    models::model_breakdown_card,
    pages::{history_error_card, section_header_large},
    section::section_title,
    summary::{usage_summary_sidebar, UsageSummary},
    theme::{claude_accent, codex_accent, page_accent},
    usage_points::{provider_point, source_bucket_values},
};

pub(super) fn all_time_page(app: &TokscopeApp, cx: &mut gpui::Context<TokscopeApp>) -> gpui::Div {
    let page = v_flex()
        .debug_selector(|| "all-time-page".to_string())
        .p_6()
        .gap_6()
        .child(section_header_large("All time", None, String::new(), cx));
    let Some(history) = &app.history else {
        return page.child(if let Some(error) = &app.history_error {
            history_error_card(error, cx)
        } else {
            all_time_loading(cx)
        });
    };
    let models = aggregate_model_usage(
        history
            .sources
            .iter()
            .flat_map(|source| source.models.iter()),
    );
    page.child(all_time_chart(history, cx))
        .child(model_breakdown_card(
            models,
            "All history",
            Page::AllTime,
            app.model_tables.sort(Page::AllTime),
            cx,
        ))
}

fn all_time_chart(history: &HistorySnapshot, cx: &App) -> gpui::Div {
    let data = all_time_points(history);
    let summary = all_time_summary(history);
    v_flex()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(chart_heading(cx))
        .child(
            h_flex()
                .gap_6()
                .items_start()
                .child(usage_summary_sidebar(summary, "EST. API COST", cx))
                .child(div().flex_1().min_w_0().child(provider_usage_chart(
                    data,
                    "all-time-usage",
                    cx,
                ))),
        )
}

pub(super) fn all_time_points(history: &HistorySnapshot) -> Vec<ProviderPoint> {
    let claude = history.source("claude");
    let codex = history.source("codex");
    let active: Vec<_> = history
        .usage
        .monthly
        .iter()
        .filter(|bucket| bucket.tokens > 0 || bucket.cost > 0.0)
        .filter_map(|bucket| {
            NaiveDate::parse_from_str(&format!("{}-01", bucket.key), "%Y-%m-%d").ok()
        })
        .collect();
    let (Some(first), Some(last)) = (active.iter().min(), active.iter().max()) else {
        return Vec::new();
    };
    std::iter::successors(Some(*first), |date| next_month(*date))
        .take_while(|date| date <= last)
        .map(|date| {
            let key = date.format("%Y-%m").to_string();
            provider_point(
                date.format("%B %Y").to_string(),
                date.format("%b %Y").to_string(),
                source_bucket_values(claude, |usage| &usage.monthly, &key),
                source_bucket_values(codex, |usage| &usage.monthly, &key),
            )
        })
        .collect()
}

fn next_month(date: NaiveDate) -> Option<NaiveDate> {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1)
}

pub(super) fn all_time_summary(history: &HistorySnapshot) -> UsageSummary {
    let totals = |client: &str| {
        history
            .source(client)
            .map(|source| (source.total_cost.max(0.0), source.total_tokens.max(0)))
            .unwrap_or_default()
    };
    let (claude_cost, claude_tokens) = totals("claude");
    let (codex_cost, codex_tokens) = totals("codex");
    UsageSummary::exact(claude_cost, claude_tokens, codex_cost, codex_tokens)
}

fn chart_heading(cx: &App) -> gpui::Div {
    h_flex()
        .justify_between()
        .items_center()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .size_2()
                        .rounded_full()
                        .bg(page_accent(Page::AllTime, cx)),
                )
                .child(section_title("Usage — all time by month")),
        )
        .child(
            h_flex()
                .gap_3()
                .child(legend("Codex", codex_accent(), cx))
                .child(legend("Claude Code", claude_accent(), cx)),
        )
}

fn legend(label: &'static str, color: gpui::Hsla, cx: &App) -> gpui::Div {
    h_flex()
        .gap_1p5()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(div().size_2().rounded_full().bg(color))
        .child(label)
}

fn all_time_loading(cx: &App) -> gpui::Div {
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
                .child(section_title("Usage — all time by month"))
                .child(loading_status("Scanning complete history", cx)),
        )
        .child(
            h_flex()
                .gap_6()
                .items_start()
                .child(loading_summary_sidebar(cx))
                .child(div().flex_1().min_w_0().child(loading_plot(280., cx))),
        )
}
