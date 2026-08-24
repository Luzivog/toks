use std::ops::Range;

use gpui::{div, prelude::*};
use gpui_component::{h_flex, ActiveTheme, StyledExt};
use toks_core::accounts::AccountId;

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
    pub(super) fn plain(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    fn toned(mut self, text: &str, tone: EventTone) -> Self {
        let start = self.text.len();
        self.text.push_str(text);
        self.tones.push((start..self.text.len(), tone, None));
        self
    }

    pub(super) fn thread(self, text: &str) -> Self {
        self.toned(text, EventTone::Thread)
    }

    pub(super) fn account(mut self, text: &str, account: &AccountId) -> Self {
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
