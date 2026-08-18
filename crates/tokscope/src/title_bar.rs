use gpui::{div, prelude::*, px, MouseButton, Window, WindowControlArea};
use gpui_component::{
    button::{Button, ButtonVariants},
    ActiveTheme, Sizable, StyledExt,
};

use crate::{
    ui,
    window::{icon_element, TokscopeIcon},
    window_controls::window_controls,
    Page, TokscopeApp,
};

pub(super) fn title_bar(
    app: &TokscopeApp,
    window: &Window,
    cx: &mut gpui::Context<TokscopeApp>,
) -> gpui::Div {
    let (page_title, page_accent) = match app.page {
        Page::Overview => ("Overview", ui::page_accent(Page::Overview, cx)),
        Page::Hourly => ("Hourly", ui::page_accent(Page::Hourly, cx)),
        Page::Daily => ("Daily", ui::page_accent(Page::Daily, cx)),
        Page::Monthly => ("Monthly", ui::page_accent(Page::Monthly, cx)),
    };
    let sidebar_tooltip = if app.sidebar_open {
        "Hide sidebar"
    } else {
        "Show sidebar"
    };
    let sidebar_icon = if app.sidebar_open {
        TokscopeIcon::PanelLeftClose
    } else {
        TokscopeIcon::PanelLeftOpen
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(48.))
        .flex_shrink_0()
        .bg(cx.theme().background)
        .child(
            div()
                .flex()
                .items_center()
                .h_full()
                .w(px(112.))
                .flex_shrink_0()
                .pl_3()
                .child(
                    Button::new("toggle-sidebar")
                        .debug_selector(|| "toggle-sidebar".to_string())
                        .ghost()
                        .small()
                        .child(
                            icon_element(sidebar_icon)
                                .size(px(16.))
                                .text_color(gpui::white()),
                        )
                        .cursor_pointer()
                        .text_color(cx.theme().foreground)
                        .tooltip(sidebar_tooltip)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.sidebar_open = !app.sidebar_open;
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("window-drag-region")
                .flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .window_control_area(WindowControlArea::Drag)
                .when(cfg!(target_os = "linux"), |region| {
                    // GPUI 0.2.2 does not route WindowControlArea on Linux.
                    region.on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        window.start_window_move();
                    })
                })
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_lg()
                        .text_sm()
                        .font_semibold()
                        .bg(cx.theme().sidebar_accent)
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(div().size(px(6.)).rounded_full().bg(page_accent))
                                .child(page_title),
                        ),
                ),
        )
        .child(window_controls(window, cx))
}
