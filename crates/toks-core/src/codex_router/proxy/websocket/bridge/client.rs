use axum::extract::ws::{Message, WebSocket};
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message as ServerMessage;

use super::message::to_server;
use super::{admission, request, usage_limit, Turn};
use crate::accounts::AccountId;
use crate::codex_router::proxy::lease::{StreamLease, ThreadAttachment};
use crate::codex_router::proxy::protocol::{
    requested_settings, ClientRequestFrame, ThreadIdentity, RETRY_FRAME,
};
use crate::codex_router::proxy::websocket::connect::UpstreamSocket;
use crate::codex_router::proxy::Engine;

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
                let request_settings = requested_settings(text);
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
                    Ok(crate::codex_router::proxy::engine::RouteSelection::Selected(selected))
                        if &selected != account =>
                    {
                        client.send(Message::Text(RETRY_FRAME.into())).await.ok()?;
                        return None;
                    }
                    Ok(crate::codex_router::proxy::engine::RouteSelection::ResumeDenied) => {
                        admission::reject(client).await?;
                        return None;
                    }
                    Ok(crate::codex_router::proxy::engine::RouteSelection::Unavailable) => {
                        usage_limit::wait(engine, &requested_thread);
                        admission::reject(client).await?;
                        return None;
                    }
                    Ok(crate::codex_router::proxy::engine::RouteSelection::Selected(_)) => {}
                    Err(_) => return None,
                }
                attach(engine, account, turn, requested_thread.as_ref(), client).await?;
                turn.thread = requested_thread;
                open_lease(engine, account, turn, &request_settings, client).await?;
                turn.active = true;
                turn.delivered = false;
                turn.lifecycle.reset();
                let request = request::prepare(engine, turn.lease.as_ref(), text);
                turn.begin_request(&request.forwarded, request.origin);
                turn.forced_fast_request = request.fallback;
                return upstream
                    .send(ServerMessage::Text(request.forwarded.into()))
                    .await
                    .ok();
            }
            ClientRequestFrame::ResponseCancel => {
                if let Some(thread) = &turn.thread {
                    engine.cancel_task(thread);
                }
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
    request_settings: &crate::rotation::ThreadRequestSettings,
    client: &mut WebSocket,
) -> Option<()> {
    let Some(thread) = turn.thread.as_ref() else {
        return Some(());
    };
    if turn.lease.is_some() {
        return Some(());
    }
    match StreamLease::open_observed(
        engine.clone(),
        account,
        thread,
        turn.resume_attempt.as_deref(),
        request_settings,
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
