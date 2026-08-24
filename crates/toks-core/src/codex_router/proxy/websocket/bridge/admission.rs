use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};

use super::super::super::engine::{Engine, RouteSelection};
use super::super::super::protocol::{ALL_UNAVAILABLE_FRAME, BAD_THREAD_FRAME};
use crate::accounts::AccountId;
use crate::rotation::ThreadId;

pub(super) fn select(
    engine: &Engine,
    thread: Option<&ThreadId>,
    resume_attempt: Option<&str>,
) -> Result<RouteSelection<AccountId>> {
    match thread {
        Some(thread) => engine.eligible_account_for_thread_authorized(thread, resume_attempt),
        None if resume_attempt.is_some() => Ok(RouteSelection::ResumeDenied),
        None => Ok(engine
            .eligible_account()?
            .map_or(RouteSelection::Unavailable, RouteSelection::Selected)),
    }
}

pub(in crate::codex_router::proxy::websocket) async fn reject(
    client: &mut WebSocket,
) -> Option<()> {
    client
        .send(Message::Text(ALL_UNAVAILABLE_FRAME.into()))
        .await
        .ok()
}

pub(in crate::codex_router::proxy::websocket) async fn reject_thread(
    client: &mut WebSocket,
) -> Option<()> {
    client
        .send(Message::Text(BAD_THREAD_FRAME.into()))
        .await
        .ok()
}
