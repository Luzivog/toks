use std::ops::Range;

use gpui::{div, prelude::*, FontWeight, HighlightStyle, SharedString, StyledText};
use gpui_component::{h_flex, tooltip::Tooltip, ActiveTheme};
use toks_core::rotation::{RotationEvent, RotationEventKind};

use crate::ToksApp;

use super::{card, empty_row, format::account_label};

pub(super) fn events_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let events = app.rotation.runtime.events();
    let meta = if events.len() > 10 {
        "Latest 10".to_string()
    } else {
        events.len().to_string()
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
                .truncate()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(event_text(app, event).styled(cx)),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventTone {
    Thread,
    Account,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct EventText {
    text: String,
    tones: Vec<(Range<usize>, EventTone)>,
}

impl EventText {
    fn plain(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    fn toned(mut self, text: &str, tone: EventTone) -> Self {
        let start = self.text.len();
        self.text.push_str(text);
        self.tones.push((start..self.text.len(), tone));
        self
    }

    fn thread(self, text: &str) -> Self {
        self.toned(text, EventTone::Thread)
    }

    fn account(self, text: &str) -> Self {
        self.toned(text, EventTone::Account)
    }

    fn styled(self, cx: &gpui::App) -> StyledText {
        let thread = cx.theme().foreground.opacity(0.72);
        let account = cx.theme().foreground;
        StyledText::new(self.text).with_highlights(self.tones.into_iter().map(
            move |(range, tone)| {
                let style = match tone {
                    EventTone::Thread => HighlightStyle::color(thread),
                    EventTone::Account => HighlightStyle {
                        color: Some(account),
                        font_weight: Some(FontWeight::MEDIUM),
                        ..Default::default()
                    },
                };
                (range, style)
            },
        ))
    }
}

fn event_text(app: &ToksApp, event: &RotationEvent) -> EventText {
    match &event.event {
        RotationEventKind::Routed {
            thread_id,
            account_id,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" routed to ")
            .account(&account_label(app, account_id)),
        RotationEventKind::Rotated {
            thread_id,
            from,
            to,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" moved from ")
            .account(&account_label(app, from))
            .plain(" to ")
            .account(&account_label(app, to)),
        RotationEventKind::Blocked { account_id, .. } => EventText::default()
            .account(&account_label(app, account_id))
            .plain(" blocked"),
        RotationEventKind::Draining { account_id } => EventText::default()
            .account(&account_label(app, account_id))
            .plain(" reached 0% and is draining"),
        RotationEventKind::AuthNeeded { account_id } => EventText::default()
            .account(&account_label(app, account_id))
            .plain(" needs sign-in"),
        RotationEventKind::Waiting { thread_id } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" is waiting"),
        RotationEventKind::Resumed {
            thread_id,
            account_id,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" resumed on ")
            .account(&account_label(app, account_id)),
        RotationEventKind::RouterFailure => EventText::default().plain("Router failure recorded"),
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
