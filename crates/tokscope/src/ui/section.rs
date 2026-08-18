use gpui::{div, prelude::*, App, SharedString};
use gpui_component::{ActiveTheme, StyledExt};

/// Standard typography for a title that belongs to a bordered section.
pub(super) fn section_title(title: impl Into<SharedString>) -> gpui::Div {
    div().text_sm().font_semibold().child(title.into())
}

/// Standard right-aligned context shown in a section header.
pub(super) fn section_meta(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}
