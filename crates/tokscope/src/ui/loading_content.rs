use gpui::{div, prelude::*, px, relative, App, Hsla};
use gpui_component::{h_flex, skeleton::Skeleton, v_flex, ActiveTheme};
use tokscope_core::history::UsagePeriod;

use super::{
    loading_plot, loading_status, loading_summary_sidebar, section_title, summary_chart_row,
    usage_chart_identity,
};

pub(super) fn table_loading_card(title: &'static str, rows: usize, cx: &App) -> gpui::Div {
    let mut card = v_flex()
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
                .child(section_title(title))
                .child(loading_status("Preparing details", cx)),
        );
    for index in 0..rows {
        card = card.child(
            h_flex()
                .gap_3()
                .py_1p5()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    Skeleton::new()
                        .w(relative(if index % 2 == 0 { 0.34 } else { 0.26 }))
                        .h_4()
                        .rounded_md(),
                )
                .child(Skeleton::new().secondary().flex_1().h_4().rounded_md()),
        );
    }
    card
}

pub(super) fn usage_chart_loading_card(period: UsagePeriod, accent: Hsla, cx: &App) -> gpui::Div {
    let summary = loading_summary_sidebar(cx);
    let (title, _) = usage_chart_identity(period);
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
                .child(loading_status("Scanning local usage", cx)),
        )
        .child(summary_chart_row(summary, loading_plot(280., cx)))
}

pub(super) fn usage_page_loading(period: UsagePeriod, accent: Hsla, cx: &App) -> gpui::Div {
    v_flex()
        .gap_6()
        .child(usage_chart_loading_card(period, accent, cx))
        .child(table_loading_card("Model breakdown", 4, cx))
        .child(table_loading_card(
            match period {
                UsagePeriod::Hourly => "Hourly usage",
                UsagePeriod::Daily => "Daily usage",
                UsagePeriod::Monthly => "Monthly usage",
            },
            8,
            cx,
        ))
}

pub(super) fn account_limits_loading_content(cx: &App) -> gpui::Div {
    let mut content = v_flex().w_full();
    for _ in 0..2 {
        let mut group = v_flex().border_t_1().border_color(cx.theme().border);
        group = group.child(
            h_flex()
                .px_3()
                .py_2p5()
                .justify_between()
                .items_center()
                .child(loading_status("Finding account", cx))
                .child(loading_status("Checking limits", cx)),
        );
        for width in [0.36_f32, 0.48] {
            group = group.child(
                h_flex()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(Skeleton::new().secondary().w(px(190.)).h_3().rounded_md())
                    .child(Skeleton::new().flex_1().h(px(6.)).rounded_full())
                    .child(
                        Skeleton::new()
                            .w(relative(width))
                            .max_w(px(140.))
                            .h_3()
                            .rounded_md(),
                    )
                    .child(Skeleton::new().secondary().w(px(120.)).h_3().rounded_md()),
            );
        }
        content = content.child(group);
    }
    content
}
