use axum::extract::ws::{Message, WebSocket};
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message as ServerMessage;

use super::super::super::lease::{StreamLease, ThreadAttachment};
use super::super::super::protocol::{ClientRequestFrame, ThreadIdentity, RETRY_FRAME};
use super::super::super::Engine;
use super::super::connect::UpstreamSocket;
use super::message::to_server;
use super::{admission, usage_limit, Turn};
use crate::accounts::AccountId;
use crate::rotation::UsageLimitTierOrigin;

mod upgrade;
use upgrade::upgraded_request;

pub(super) async fn handle(
    client: &mut WebSocket,
    upstream: &mut UpstreamSocket,
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    message: Message,
) -> Option<()> {
    if matches!(message, Message::Binary(_)) {
        admission::reject_thread(client).await?;
        return None;
    }
    if let Message::Text(text) = &message {
        match ClientRequestFrame::from_payload(text.as_bytes()) {
            ClientRequestFrame::Denied => {
                admission::reject_thread(client).await?;
                return None;
            }
            ClientRequestFrame::ResponseCreate(payload_identity) => {
                let identity = turn
                    .header_thread
                    .clone()
                    .map_or(ThreadIdentity::Absent, ThreadIdentity::Unique)
                    .merge(payload_identity);
                if identity == ThreadIdentity::Denied {
                    admission::reject_thread(client).await?;
                    return None;
                }
                let requested_thread = identity.into_thread().or_else(|| turn.thread.clone());
                if turn.active {
                    admission::reject_thread(client).await?;
                    return None;
                }
                match admission::select(
                    engine,
                    requested_thread.as_ref(),
                    turn.resume_attempt.as_deref(),
                ) {
                    Ok(super::super::super::engine::RouteSelection::Selected(selected))
                        if &selected != account =>
                    {
                        client.send(Message::Text(RETRY_FRAME.into())).await.ok()?;
                        return None;
                    }
                    Ok(super::super::super::engine::RouteSelection::ResumeDenied) => {
                        admission::reject(client).await?;
                        return None;
                    }
                    Ok(super::super::super::engine::RouteSelection::Unavailable) => {
                        usage_limit::wait(engine, &requested_thread);
                        admission::reject(client).await?;
                        return None;
                    }
                    Ok(super::super::super::engine::RouteSelection::Selected(_)) => {}
                    Err(_) => return None,
                }
                attach(engine, account, turn, requested_thread.as_ref(), client).await?;
                turn.thread = requested_thread;
                open_lease(engine, account, turn, client).await?;
                turn.active = true;
                turn.delivered = false;
                turn.forced_fast_request = None;
                turn.lifecycle.reset();
                if let Some((upgraded, fallback, origin)) = upgraded_request(engine, turn, text) {
                    turn.begin_request(&upgraded, origin);
                    turn.forced_fast_request = fallback;
                    return upstream
                        .send(ServerMessage::Text(upgraded.into()))
                        .await
                        .ok();
                }
                turn.begin_request(text, UsageLimitTierOrigin::Client);
            }
            ClientRequestFrame::Other => {}
        }
    }
    upstream.send(to_server(message)).await.ok()
}

async fn attach(
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    thread: Option<&crate::rotation::ThreadId>,
    client: &mut WebSocket,
) -> Option<()> {
    let Some(thread) = thread else {
        return Some(());
    };
    if turn
        .attachment
        .as_ref()
        .is_some_and(|attached| attached.matches(thread))
    {
        return Some(());
    }
    turn.attachment = None;
    match ThreadAttachment::open(
        engine.clone(),
        account,
        thread,
        turn.resume_attempt.as_deref(),
    ) {
        Ok(Some(attachment)) => {
            turn.attachment = Some(attachment);
            Some(())
        }
        Ok(None) => retry(client).await,
        Err(_) => None,
    }
}

async fn open_lease(
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    client: &mut WebSocket,
) -> Option<()> {
    let Some(thread) = turn.thread.as_ref() else {
        return Some(());
    };
    if turn.lease.is_some() {
        return Some(());
    }
    match StreamLease::open(
        engine.clone(),
        account,
        thread,
        turn.resume_attempt.as_deref(),
    ) {
        Ok(Some(lease)) => {
            turn.lease = Some(lease);
            Some(())
        }
        Ok(None) => retry(client).await,
        Err(_) => None,
    }
}

async fn retry(client: &mut WebSocket) -> Option<()> {
    let _ = client.send(Message::Text(RETRY_FRAME.into())).await;
    None
}
