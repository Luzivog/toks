use gpui::{div, prelude::*, App};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use tokscope_core::history::UsagePeriod;

use crate::{Page, TokscopeApp};

use super::{
    account_limits_section, breakdown_card, model_breakdown_card, overview_history_loading,
    page_accent, period_model_usage, table_loading_card, usage_block, usage_chart_card,
    usage_history_card, usage_page_loading, usage_period_label,
};

pub(crate) fn detail(app: &TokscopeApp, cx: &mut gpui::Context<TokscopeApp>) -> impl IntoElement {
    let page = app.page;
    let body = match page {
        Page::Overview => overview_page(app, cx),
        Page::AllTime => super::all_time::all_time_page(app, cx),
        _ => usage_page(app, page.usage_period().expect("usage page period"), cx),
    };
    div()
        .id("detail")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(body)
}

pub(super) fn overview_page(app: &TokscopeApp, cx: &mut gpui::Context<TokscopeApp>) -> gpui::Div {
    let mut root = v_flex().p_6().gap_5();

    // Historical summary first, followed by current plan state. The detailed
    // cross-provider breakdown is intentionally the final overview section.
    if let Some(history) = &app.history {
        root = root.child(usage_block(history, cx));
    } else if let Some(error) = &app.history_error {
        root = root.child(history_error_card(error, cx));
    } else {
        root = root.child(overview_history_loading(cx));
    }

    root = root.child(account_limits_section(app, "Usage remaining", cx));
    if let Some(history) = &app.history {
        root = root.child(breakdown_card(history, app, cx));
    } else if app.history_error.is_none() {
        root = root.child(table_loading_card("Model breakdown", 5, cx));
    }

    root
}

pub(super) fn usage_page(
    app: &TokscopeApp,
    period: UsagePeriod,
    cx: &mut gpui::Context<TokscopeApp>,
) -> gpui::Div {
    let mut root = v_flex().p_6().gap_6();
    root = root.child(section_header_large(
        usage_period_label(period),
        None,
        String::new(),
        cx,
    ));

    if let Some(history) = &app.history {
        let (models, range) = match period {
            UsagePeriod::Hourly => (period_model_usage(history, period), "Last 60 minutes"),
            UsagePeriod::Daily => (period_model_usage(history, period), "Today"),
            UsagePeriod::Monthly => (period_model_usage(history, period), "This month"),
        };
        root = root
            .child(usage_chart_card(
                history,
                period,
                page_accent(app.page, cx),
                cx,
            ))
            .child(model_breakdown_card(
                models,
                range,
                app.page,
                app.model_tables.sort(app.page),
                cx,
            ))
            .child(usage_history_card(
                history,
                period,
                app.usage_tables.sort(period),
                app.usage_tables.visible_limit(period),
                cx,
            ));
    } else if let Some(error) = &app.history_error {
        root = root.child(history_error_card(error, cx));
    } else {
        root = root.child(usage_page_loading(period, page_accent(app.page, cx), cx));
    }

    root
}

pub(super) fn section_header_large(
    title: &'static str,
    plan: Option<&str>,
    right: String,
    cx: &App,
) -> gpui::Div {
    header_impl(title, plan, right, true, cx)
}

pub(super) fn header_impl(
    title: &'static str,
    plan: Option<&str>,
    right: String,
    large: bool,
    cx: &App,
) -> gpui::Div {
    h_flex()
        .items_center()
        .justify_between()
        .child(
            h_flex()
                .gap_3()
                .items_center()
                .child(if large {
                    div().text_2xl().font_bold().child(title)
                } else {
                    div().text_lg().font_semibold().child(title)
                })
                .when_some(plan, |d, p| {
                    d.child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .text_xs()
                            .font_semibold()
                            .bg(cx.theme().secondary)
                            .text_color(cx.theme().secondary_foreground)
                            .child(p.to_uppercase()),
                    )
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(right),
        )
}

pub(super) fn history_error_card(error: &str, cx: &App) -> gpui::Div {
    v_flex()
        .gap_2()
        .p_4()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().danger.opacity(0.45))
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .child(div().size_2().rounded_full().bg(cx.theme().danger))
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child("Couldn't load usage history"),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(error.to_string()),
        )
}
