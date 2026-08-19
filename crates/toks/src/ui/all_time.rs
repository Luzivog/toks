use chrono::{Local, TimeZone};
use gpui::{div, prelude::*, App};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::history::HistorySnapshot;

use crate::{Page, ToksApp};

use super::{
    all_time_data::{all_time_points, all_time_summary},
    chart_layout::summary_chart_row,
    chart_plot::provider_usage_chart,
    history_freshness_text,
    loading_chart::{loading_plot, loading_status, loading_summary_sidebar},
    model_data::aggregate_model_usage,
    models::model_breakdown_card,
    pages::{history_error_card, section_header_large},
    section::section_title,
    summary::usage_summary_sidebar,
    theme::{claude_accent, codex_accent, page_accent},
};

pub(super) fn all_time_page(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let mut capture = app
        .history
        .as_ref()
        .and_then(capture_label)
        .unwrap_or_default();
    if let Some(refresh) = history_freshness_text(&app.history_refresh, app.now) {
        if !capture.is_empty() {
            capture.push_str(" · ");
        }
        capture.push_str(&refresh);
    }
    let page = v_flex()
        .debug_selector(|| "all-time-page".to_string())
        .p_6()
        .gap_6()
        .child(section_header_large("All time", None, capture, cx));
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

fn capture_label(history: &HistorySnapshot) -> Option<String> {
    let mut parts = Local
        .timestamp_millis_opt(history.captured_since_ms?)
        .single()
        .map(|captured| {
            vec![format!(
                "Recorded locally since {}",
                captured.format("%b %-d, %Y")
            )]
        })?;
    if history.weak_events > 0 {
        parts.push(format!(
            "{} best-effort {}",
            history.weak_events,
            plural(history.weak_events, "record", "records")
        ));
    }
    if history.history_conflicts > 0 {
        parts.push(format!(
            "{} {} preserved",
            history.history_conflicts,
            plural(history.history_conflicts, "conflict", "conflicts")
        ));
    }
    Some(parts.join(" · "))
}

fn plural<'a>(count: i64, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
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
        .child(summary_chart_row(
            usage_summary_sidebar(summary, "EST. API COST", cx),
            provider_usage_chart(data, "all-time-usage", cx),
        ))
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
                .child(section_title("Usage — all time by week")),
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
                .child(section_title("Usage — all time by week"))
                .child(loading_status("Scanning complete history", cx)),
        )
        .child(summary_chart_row(
            loading_summary_sidebar(cx),
            loading_plot(cx),
        ))
}
