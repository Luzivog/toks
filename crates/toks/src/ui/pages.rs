use gpui::{div, prelude::*, px, App, Pixels};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::history::UsagePeriod;

use crate::{Page, ToksApp};

use super::{
    account_limits_section, history_error_card, history_freshness_text, model_breakdown_card,
    overview_history_loading, page_accent, period_model_usage, usage_block, usage_chart_card,
    usage_history_card, usage_page_loading, usage_period_label, TableLayout,
    PAGE_CONTENT_MAX_WIDTH,
};

pub(crate) fn detail(
    app: &ToksApp,
    detail_width: Pixels,
    cx: &mut gpui::Context<ToksApp>,
) -> impl IntoElement {
    let page = app.page;
    let layout = TableLayout::from_detail_width(detail_width);
    let body = match page {
        Page::Overview => overview_page(app, layout, cx),
        Page::AllTime => super::all_time::all_time_page(app, layout, cx),
        Page::Rotation => super::rotation::rotation_page(app, cx),
        _ => usage_page(
            app,
            page.usage_period().expect("usage page period"),
            layout,
            cx,
        ),
    };
    div()
        .id("detail")
        .debug_selector(|| "detail".to_string())
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(
            div().w_full().flex().justify_center().child(
                div()
                    .debug_selector(|| "page-content".to_string())
                    .w_full()
                    .max_w(px(PAGE_CONTENT_MAX_WIDTH))
                    .child(body),
            ),
        )
}

pub(super) fn overview_page(
    app: &ToksApp,
    layout: TableLayout,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let mut root = v_flex().p_6().gap_5();

    // Keep the Overview focused on time ranges and current plan state. Detailed
    // model attribution remains available on the scoped usage pages.
    if let Some(history) = &app.history {
        root = root.child(usage_block(
            history,
            history_freshness_text(&app.history_refresh, app.now),
            layout,
            cx,
        ));
    } else if let Some(error) = &app.history_error {
        root = root.child(history_error_card(error, cx));
    } else {
        root = root.child(overview_history_loading(cx));
    }

    root = root.child(account_limits_section(app, "Usage remaining", cx));
    root
}

pub(super) fn usage_page(
    app: &ToksApp,
    period: UsagePeriod,
    layout: TableLayout,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let mut root = v_flex().p_6().gap_6();
    let refresh = history_freshness_text(&app.history_refresh, app.now).unwrap_or_default();
    root = root.child(section_header_large(
        usage_period_label(period),
        None,
        refresh,
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
                layout,
                cx,
            ))
            .child(usage_history_card(
                history,
                period,
                app.usage_tables.sort(period),
                app.usage_tables.visible_limit(period),
                history_freshness_text(&app.history_refresh, app.now),
                layout,
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
