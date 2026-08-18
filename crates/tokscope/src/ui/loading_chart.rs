use gpui::{div, prelude::*, px, relative, App};
use gpui_component::{h_flex, skeleton::Skeleton, v_flex, ActiveTheme};

use crate::Page;

use super::{page_accent, section_title};

pub(super) fn loading_status(text: &'static str, cx: &App) -> gpui::Div {
    h_flex()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(Skeleton::new().size(px(7.)).rounded_full())
        .child(text)
}

pub(super) fn loading_plot(height: f32, cx: &App) -> gpui::Div {
    let mut plot = div().relative().w_full().h(px(height)).overflow_hidden();
    for top in [0.08_f32, 0.31, 0.54, 0.77, 1.0] {
        plot = plot.child(
            div()
                .absolute()
                .top(relative(top))
                .left_0()
                .right_0()
                .h(px(1.))
                .bg(cx.theme().border.opacity(0.7)),
        );
    }

    let mut bars = h_flex()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(8.))
        .h(relative(0.72))
        .items_end()
        .gap_2();
    for bar_height in [
        0.22_f32, 0.46, 0.31, 0.68, 0.42, 0.78, 0.53, 0.35, 0.61, 0.27,
    ] {
        bars = bars.child(
            Skeleton::new()
                .secondary()
                .flex_1()
                .h(relative(bar_height))
                .rounded_t_md(),
        );
    }
    plot.child(bars)
}

fn overview_range_loading_card(title: &'static str, page: Page, cx: &App) -> gpui::Div {
    let summary = loading_summary_sidebar(cx);

    v_flex()
        .flex_1()
        .min_w(px(700.))
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
                        .child(div().size_2().rounded_full().bg(page_accent(page, cx)))
                        .child(section_title(title)),
                )
                .child(loading_status("Scanning local usage", cx)),
        )
        .child(
            h_flex()
                .gap_6()
                .items_start()
                .child(summary)
                .child(div().flex_1().min_w_0().child(loading_plot(280., cx))),
        )
}

pub(super) fn loading_summary_sidebar(_cx: &App) -> gpui::Div {
    let mut providers = v_flex().gap_5();
    for width in [0.84_f32, 0.63] {
        providers = providers.child(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .justify_between()
                        .child(Skeleton::new().w(relative(0.36)).h_4().rounded_md())
                        .child(Skeleton::new().w(relative(0.28)).h_4().rounded_md()),
                )
                .child(Skeleton::new().w_full().h_2().rounded_full())
                .child(
                    Skeleton::new()
                        .secondary()
                        .w(relative(width))
                        .h_3()
                        .rounded_md(),
                ),
        );
    }

    v_flex()
        .w(px(290.))
        .flex_shrink_0()
        .gap_6()
        .child(
            h_flex()
                .gap_5()
                .child(Skeleton::new().flex_1().h(px(58.)).rounded_md())
                .child(Skeleton::new().w(px(100.)).h(px(58.)).rounded_md()),
        )
        .child(providers)
}

pub(super) fn overview_history_loading(cx: &App) -> gpui::Div {
    h_flex()
        .w_full()
        .flex_wrap()
        .gap_4()
        .child(overview_range_loading_card("Today", Page::Daily, cx))
        .child(overview_range_loading_card("This month", Page::Monthly, cx))
}
