use gpui::{div, prelude::*, px};
use gpui_component::{button::Button, h_flex, v_flex, ActiveTheme, StyledExt};

use crate::{Page, ToksApp};

use super::{action_button, page_accent};

pub(crate) fn sidebar(
    app: &ToksApp,
    cx: &mut gpui::Context<ToksApp>,
    overlay: bool,
) -> impl IntoElement {
    let mut sidebar = v_flex()
        .w(px(250.))
        .h_full()
        .flex_shrink_0()
        .bg(cx.theme().sidebar)
        .border_r_1()
        .border_color(cx.theme().sidebar_border)
        .child(
            div()
                .h(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .px_4()
                .text_sm()
                .font_semibold()
                .text_color(cx.theme().sidebar_foreground)
                .child("Usage"),
        );
    for page in Page::ALL {
        if page == Page::Overview {
            sidebar = sidebar.child(section_label("USAGE", cx));
        } else if page == Page::Rotation {
            sidebar = sidebar.child(section_label("ROUTING", cx));
        }
        sidebar = sidebar.child(sidebar_entry(app, cx, page, overlay));
    }
    sidebar
}

/// Small uppercase group heading that structures the sidebar into
/// "usage views" and "routing" without an orphaning divider.
fn section_label(title: &'static str, cx: &gpui::App) -> gpui::Div {
    div()
        .mx_4()
        .mt_3()
        .mb_1()
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().sidebar_foreground.opacity(0.45))
        .child(title)
}

pub(super) fn sidebar_entry(
    app: &ToksApp,
    cx: &mut gpui::Context<ToksApp>,
    page: Page,
    overlay: bool,
) -> Button {
    let selected = app.page() == page;
    let accent = page_accent(page, cx);
    action_button(page.slug(), cx)
        .mx_2()
        .my_0p5()
        .h(px(38.))
        .px_3()
        .rounded_lg()
        .justify_start()
        .when(selected, |d| d.bg(accent.opacity(0.12)))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.navigate_to(page);
            if overlay {
                app.sidebar_open = false;
            }
            cx.notify();
        }))
        .child(
            h_flex()
                .gap_3()
                .items_center()
                .child(div().size_2().rounded_full().bg(accent).flex_shrink_0())
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(cx.theme().sidebar_foreground)
                        .child(page.title()),
                ),
        )
}
