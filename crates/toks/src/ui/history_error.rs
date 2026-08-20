use gpui::{div, prelude::*, App};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};

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
