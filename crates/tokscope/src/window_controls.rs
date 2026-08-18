use gpui::{div, prelude::*, px, Window, WindowControlArea};
use gpui_component::ActiveTheme;

use crate::window::{WindowAction, WindowFrame};

fn maximize_icon(window: &Window, cx: &gpui::App) -> gpui::Div {
    let foreground = cx.theme().foreground;
    if window.is_maximized() {
        div()
            .relative()
            .size(px(12.))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(8.))
                    .rounded(px(1.))
                    .border_1()
                    .border_color(foreground),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .size(px(8.))
                    .rounded(px(1.))
                    .border_1()
                    .border_color(foreground)
                    .bg(cx.theme().background),
            )
    } else {
        div()
            .size(px(10.))
            .rounded(px(1.))
            .border_1()
            .border_color(foreground)
    }
}

pub(super) fn window_controls(window: &Window, cx: &gpui::App) -> gpui::Div {
    let foreground = cx.theme().foreground;
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .w(px(112.))
        .flex_shrink_0()
        .gap_2()
        .pr_3()
        .child(
            div()
                .id("minimize")
                .debug_selector(|| "window-minimize".to_string())
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded_full()
                .cursor_pointer()
                .text_color(foreground)
                .text_lg()
                .hover(|button| button.bg(cx.theme().sidebar_accent))
                .window_control_area(WindowControlArea::Min)
                .when(!cfg!(target_os = "windows"), |button| {
                    button.on_click(|_, window, cx| {
                        cx.stop_propagation();
                        WindowFrame::perform_window_action(WindowAction::Minimize, window, cx);
                    })
                })
                .child("−"),
        )
        .child(
            div()
                .id("maximize")
                .debug_selector(|| "window-maximize".to_string())
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded_full()
                .cursor_pointer()
                .text_color(foreground)
                .hover(|button| button.bg(cx.theme().sidebar_accent))
                .window_control_area(WindowControlArea::Max)
                .when(!cfg!(target_os = "windows"), |button| {
                    button.on_click(|_, window, cx| {
                        cx.stop_propagation();
                        WindowFrame::perform_window_action(
                            WindowAction::ToggleMaximize,
                            window,
                            cx,
                        );
                    })
                })
                .child(maximize_icon(window, cx)),
        )
        .child(
            div()
                .id("close")
                .debug_selector(|| "window-close".to_string())
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded_full()
                .cursor_pointer()
                .text_color(foreground)
                .text_lg()
                .hover(|button| {
                    button
                        .bg(cx.theme().danger)
                        .text_color(cx.theme().danger_foreground)
                })
                .window_control_area(WindowControlArea::Close)
                .when(!cfg!(target_os = "windows"), |button| {
                    button.on_click(|_, window, cx| {
                        cx.stop_propagation();
                        WindowFrame::perform_window_action(WindowAction::Close, window, cx);
                    })
                })
                .child("×"),
        )
}
