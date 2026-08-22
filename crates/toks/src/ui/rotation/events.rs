use gpui::{div, prelude::*, SharedString};
use gpui_component::{h_flex, tooltip::Tooltip, ActiveTheme};
use toks_core::rotation::RotationEvent;

use crate::ToksApp;

use super::{card, empty_row};

mod text;
use text::event_text;
#[cfg(test)]
use text::EventTone;

pub(super) fn events_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let events = app.rotation.runtime.events();
    let count = events.len();
    let meta = if count > 10 {
        "Latest 10".into()
    } else {
        count.to_string()
    };
    let mut panel = card("Recent activity", meta, cx);
    if events.is_empty() {
        return panel.child(empty_row("No routing activity yet.", cx));
    }
    for event in events.iter().take(10) {
        panel = panel.child(event_row(app, event, cx));
    }
    panel
}

fn event_row(app: &ToksApp, event: &RotationEvent, cx: &gpui::App) -> gpui::Div {
    let time_selector = format!("rotation-event-time-{}", event.at.get());
    let tooltip_selector = format!("rotation-event-time-tooltip-{}", event.at.get());
    let exact = super::format::exact_time(event.at);
    h_flex()
        .gap_3()
        .items_center()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(event_text(app, event).render(app, event.at.get(), cx)),
        )
        .child(
            div()
                .id(SharedString::from(time_selector.clone()))
                .debug_selector(move || time_selector.clone())
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(super::format::age(app.now, event.at))
                .tooltip(move |window, cx| {
                    let exact = exact.clone();
                    let selector = tooltip_selector.clone();
                    Tooltip::element(move |_, _| {
                        let selector = selector.clone();
                        div()
                            .debug_selector(move || selector.clone())
                            .child(exact.clone())
                    })
                    .build(window, cx)
                }),
        )
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
