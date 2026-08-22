use std::ops::Range;

use gpui::{div, prelude::*};
use gpui_component::{h_flex, ActiveTheme, StyledExt};
use toks_core::accounts::AccountId;
use toks_core::rotation::{RotationEvent, RotationEventKind};

use crate::ToksApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventTone {
    Thread,
    Account,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct EventText {
    pub(super) text: String,
    pub(super) tones: Vec<(Range<usize>, EventTone, Option<AccountId>)>,
}

impl EventText {
    fn plain(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    fn toned(mut self, text: &str, tone: EventTone) -> Self {
        let start = self.text.len();
        self.text.push_str(text);
        self.tones.push((start..self.text.len(), tone, None));
        self
    }

    fn thread(self, text: &str) -> Self {
        self.toned(text, EventTone::Thread)
    }

    fn account(mut self, text: &str, account: &AccountId) -> Self {
        let start = self.text.len();
        self.text.push_str(text);
        self.tones.push((
            start..self.text.len(),
            EventTone::Account,
            Some(account.clone()),
        ));
        self
    }

    pub(super) fn render(self, app: &ToksApp, event_at: i64, cx: &gpui::App) -> gpui::Div {
        let thread = cx.theme().foreground.opacity(0.72);
        let account = cx.theme().foreground;
        let mut row = h_flex().min_w_0().overflow_hidden().text_sm();
        let mut cursor = 0;
        for (range, tone, account_id) in self.tones {
            if cursor < range.start {
                row = row.child(
                    div()
                        .whitespace_nowrap()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.text[cursor..range.start].to_string()),
                );
            }
            let text = self.text[range.clone()].to_string();
            row = match (tone, account_id) {
                (EventTone::Thread, _) => {
                    row.child(div().whitespace_nowrap().text_color(thread).child(text))
                }
                (EventTone::Account, Some(account_id)) => {
                    row.child(super::super::format::account_identity(
                        app,
                        &account_id,
                        &format!("rotation-event-{event_at}"),
                        div().whitespace_nowrap().font_medium().text_color(account),
                        cx,
                    ))
                }
                (EventTone::Account, None) => row,
            };
            cursor = range.end;
        }
        if cursor < self.text.len() {
            row = row.child(
                div()
                    .whitespace_nowrap()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.text[cursor..].to_string()),
            );
        }
        row
    }
}

pub(super) fn event_text(app: &ToksApp, event: &RotationEvent) -> EventText {
    use super::super::format::account_name;

    match &event.event {
        RotationEventKind::Routed {
            thread_id,
            account_id,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" routed to ")
            .account(&account_name(app, account_id), account_id),
        RotationEventKind::Rotated {
            thread_id,
            from,
            to,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" moved from ")
            .account(&account_name(app, from), from)
            .plain(" to ")
            .account(&account_name(app, to), to),
        RotationEventKind::Blocked { account_id, .. } => EventText::default()
            .account(&account_name(app, account_id), account_id)
            .plain(" blocked"),
        RotationEventKind::Draining { account_id } => EventText::default()
            .account(&account_name(app, account_id), account_id)
            .plain(" reached 0% and is draining"),
        RotationEventKind::AuthNeeded { account_id } => EventText::default()
            .account(&account_name(app, account_id), account_id)
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
            .account(&account_name(app, account_id), account_id),
        RotationEventKind::RouterFailure => EventText::default().plain("Router failure recorded"),
    }
}
