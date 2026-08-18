use gpui::{prelude::*, px, App, SharedString};
use gpui_component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    ActiveTheme, Sizable,
};

use crate::SortDirection;

/// The shared neutral action treatment used throughout Tokscope.
///
/// Normal state is fully transparent; hover and active states reveal the
/// clickable surface without changing the foreground color.
pub(super) fn action_button(id: impl Into<SharedString>, cx: &App) -> Button {
    let id = id.into();
    let selector = id.to_string();
    let foreground = cx.theme().foreground;
    let hover = cx.theme().sidebar_accent;
    let active = cx.theme().border;
    Button::new(id)
        .debug_selector(move || selector)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(foreground)
                .hover(hover)
                .active(active),
        )
        .small()
        .cursor_pointer()
        // The component owns hover registration; a base refinement keeps its
        // foreground neutral without installing a second hover style.
        .text_color(foreground)
}

pub(super) fn text_action(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    cx: &App,
) -> Button {
    action_button(id, cx).label(label)
}

pub(super) fn sort_action(
    id: impl Into<SharedString>,
    label: &'static str,
    width: f32,
    active: bool,
    direction: SortDirection,
    cx: &App,
) -> Button {
    let id = id.into();
    let selector = id.to_string();
    let indicator_selector = format!("{selector}-indicator");
    let label_selector = format!("{selector}-label");
    let indicator = match direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    let foreground = if active {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground
    };
    action_button(id, cx)
        .compact()
        .w(px(width))
        .h(px(24.))
        .px_1()
        .justify_end()
        .gap_1()
        .text_xs()
        .whitespace_nowrap()
        .text_color(foreground)
        .child(
            gpui::div()
                .debug_selector(move || indicator_selector.clone())
                .w(px(10.))
                .flex_shrink_0()
                .text_center()
                .when(active, |slot| slot.child(indicator)),
        )
        .child(
            gpui::div()
                .debug_selector(move || label_selector.clone())
                .child(label),
        )
}
