use axum::extract::ws::{Message as ClientMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as ServerMessage;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::lease::{StreamLease, ThreadAttachment};
use super::super::protocol::{
    is_response_create, requested_model, thread_id, with_service_tier, ResponseLifecycle,
    ALL_UNAVAILABLE_FRAME, RETRY_FRAME,
};
use super::super::{engine::RouteTier, Engine};
use super::connect::UpstreamSocket;
use message::to_server;

mod message;
mod server;
mod turn;
mod usage_limit;
use turn::Turn;

pub(super) async fn run(
    mut client: WebSocket,
    mut upstream: UpstreamSocket,
    engine: std::sync::Arc<Engine>,
    account: AccountId,
    initial_thread: Option<ThreadId>,
) {
    let attachment = match initial_thread.as_ref() {
        Some(thread) => match ThreadAttachment::open(engine.clone(), &account, thread) {
            Ok(Some(attachment)) => Some(attachment),
            Ok(None) => return,
            Err(_) => return,
        },
        None => None,
    };
    let mut turn = Turn {
        active: false,
        delivered: false,
        thread: initial_thread,
        attachment,
        lease: None,
        forced_fast_request: None,
        lifecycle: ResponseLifecycle::default(),
    };
    loop {
        tokio::select! {
            client_message = client.next() => {
                let Some(Ok(message)) = client_message else { break };
                if handle_client(&mut client, &mut upstream, &engine, &account,
                    &mut turn, message).await.is_err() {
                    break;
                }
            }
            server_message = upstream.next() => {
                let Some(Ok(message)) = server_message else { break };
                if server::handle(
                    &mut client,
                    &mut upstream,
                    &engine,
                    &account,
                    &mut turn,
                    message,
                ).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn handle_client(
    client: &mut WebSocket,
    upstream: &mut UpstreamSocket,
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    message: ClientMessage,
) -> Result<(), ()> {
    if let ClientMessage::Text(text) = &message {
        if is_response_create(text) {
            let requested_thread = thread_id(text.as_bytes()).or_else(|| turn.thread.clone());
            let changes_thread = requested_thread.as_ref().is_some_and(|thread| {
                turn.attachment
                    .as_ref()
                    .is_some_and(|attached| !attached.matches(thread))
            });
            if !turn.active || changes_thread {
                let eligible = requested_thread.as_ref().map_or_else(
                    || engine.eligible_account(),
                    |thread| engine.eligible_account_for_thread(thread),
                );
                match eligible.ok().flatten() {
                    Some(selected) if &selected != account => {
                        client
                            .send(ClientMessage::Text(RETRY_FRAME.into()))
                            .await
                            .map_err(|_| ())?;
                        return Err(());
                    }
                    None => {
                        usage_limit::wait(engine, &requested_thread);
                        client
                            .send(ClientMessage::Text(ALL_UNAVAILABLE_FRAME.into()))
                            .await
                            .map_err(|_| ())?;
                        return Err(());
                    }
                    Some(_) => {}
                }
            }
            if let Some(thread) = requested_thread.as_ref() {
                if turn
                    .attachment
                    .as_ref()
                    .is_none_or(|attached| !attached.matches(thread))
                {
                    turn.attachment = None;
                    match ThreadAttachment::open(engine.clone(), account, thread) {
                        Ok(Some(attachment)) => turn.attachment = Some(attachment),
                        Ok(None) => {
                            client
                                .send(ClientMessage::Text(RETRY_FRAME.into()))
                                .await
                                .map_err(|_| ())?;
                            return Err(());
                        }
                        Err(_) => return Err(()),
                    }
                }
            }
            turn.thread = requested_thread;
            if turn.lease.is_none() {
                if let Some(id) = turn.thread.as_ref() {
                    turn.lease = match StreamLease::open(engine.clone(), account, id) {
                        Ok(Some(lease)) => Some(lease),
                        Ok(None) => {
                            client
                                .send(ClientMessage::Text(RETRY_FRAME.into()))
                                .await
                                .map_err(|_| ())?;
                            return Err(());
                        }
                        Err(_) => return Err(()),
                    };
                }
            }
            turn.active = true;
            turn.delivered = false;
            turn.forced_fast_request = None;
            turn.lifecycle.reset();
            // Burn the remainder fast, for the models that offer it.
            let upgraded = match turn
                .lease
                .as_ref()
                .map_or(RouteTier::Original, StreamLease::tier)
            {
                RouteTier::Original => None,
                RouteTier::Standard => with_service_tier(text, "default").map(|body| (body, None)),
                RouteTier::Fast => requested_model(text)
                    .and_then(|model| engine.fast_tier_for(&model))
                    .and_then(|tier| with_service_tier(text, tier))
                    .map(|body| {
                        let fallback = (body != text.as_str()).then(|| {
                            with_service_tier(text, "default").unwrap_or_else(|| text.to_string())
                        });
                        (body, fallback)
                    }),
            };
            if let Some((upgraded, fallback)) = upgraded {
                if fallback.is_some() {
                    turn.forced_fast_request = fallback;
                }
                return upstream
                    .send(ServerMessage::Text(upgraded.as_str().into()))
                    .await
                    .map_err(|_| ());
            }
        }
    }
    upstream.send(to_server(message)).await.map_err(|_| ())
}
