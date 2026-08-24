use toks_core::rotation::{RotationEvent, RotationEventKind};

use crate::ToksApp;

use super::render::EventText;

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
        RotationEventKind::ThreadBlocked {
            thread_id,
            account_id,
            ..
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" blocked on ")
            .account(&account_name(app, account_id), account_id),
        RotationEventKind::FastFallback {
            thread_id,
            account_id,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" switched to Standard on ")
            .account(&account_name(app, account_id), account_id),
        RotationEventKind::FastUnavailable {
            thread_id,
            account_id,
        } => EventText::default()
            .plain("Thread ")
            .thread(thread_id.as_str())
            .plain(" will use Standard after Fast ran out on ")
            .account(&account_name(app, account_id), account_id),
        RotationEventKind::Draining { account_id } => EventText::default()
            .account(&account_name(app, account_id), account_id)
            .plain(" reached 1% remaining and is draining"),
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
        RotationEventKind::UsageLimited {
            account_id,
            incident,
        } => {
            let mut text = EventText::default();
            if let Some(thread) = incident.thread_id() {
                text = text.plain("Thread ").thread(thread.as_str()).plain(" hit ");
            } else {
                text = text.plain("Upstream ");
            }
            text = text
                .plain(incident.tier().label())
                .plain(" usage limit on ")
                .account(&account_name(app, account_id), account_id)
                .plain(" via ")
                .plain(incident.phase().label())
                .plain(" (")
                .plain(incident.evidence().classification().label());
            if let Some(model) = incident.model() {
                text = text.plain(", ").plain(model);
            }
            text.plain(", ")
                .plain(incident.tier().origin().label())
                .plain(")")
        }
        RotationEventKind::RouterFailure => EventText::default().plain("Router failure recorded"),
    }
}
