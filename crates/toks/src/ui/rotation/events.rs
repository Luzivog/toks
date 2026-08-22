use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::rotation::{RotationEvent, RotationEventKind};

use crate::ToksApp;

use super::{card, empty_row, format::account_label};

pub(super) fn events_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let events = app.rotation.runtime.events();
    let mut panel = card("Recent activity", format!("{} / 100", events.len()), cx);
    if events.is_empty() {
        return panel.child(empty_row("No routing activity yet.", cx));
    }
    panel = panel.child(
        div()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("Metadata only. Toks does not keep prompts or responses here."),
    );
    for event in events {
        panel = panel.child(event_row(app, event, cx));
    }
    panel
}

fn event_row(app: &ToksApp, event: &RotationEvent, cx: &gpui::App) -> gpui::Div {
    h_flex()
        .min_h(px(38.))
        .gap_3()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .w(px(72.))
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(super::format::age(app.now, event.at)),
        )
        .child(
            v_flex()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().truncate().child(event_text(app, event)))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(super::format::exact_time(event.at)),
                ),
        )
}

fn event_text(app: &ToksApp, event: &RotationEvent) -> String {
    match &event.event {
        RotationEventKind::Routed {
            thread_id,
            account_id,
        } => format!(
            "Thread {} routed to {}",
            thread_id.as_str(),
            account_label(app, account_id)
        ),
        RotationEventKind::Rotated {
            thread_id,
            from,
            to,
        } => format!(
            "Thread {} moved from {} to {}",
            thread_id.as_str(),
            account_label(app, from),
            account_label(app, to)
        ),
        RotationEventKind::Blocked { account_id, until } => format!(
            "{} blocked until {}",
            account_label(app, account_id),
            super::format::exact_time(*until)
        ),
        RotationEventKind::AuthNeeded { account_id } => {
            format!("{} needs sign-in", account_label(app, account_id))
        }
        RotationEventKind::Waiting { thread_id } => {
            format!("Thread {} is waiting", thread_id.as_str())
        }
        RotationEventKind::Resumed {
            thread_id,
            account_id,
        } => format!(
            "Thread {} resumed on {}",
            thread_id.as_str(),
            account_label(app, account_id)
        ),
        RotationEventKind::RouterFailure => "Router failure recorded".into(),
    }
}
