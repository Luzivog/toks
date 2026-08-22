use axum::extract::ws::{Message as ClientMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as ServerMessage;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::lease::{StreamLease, ThreadAttachment};
use super::super::protocol::{
    is_response_create, model_visible_output, requested_model, thread_id, websocket_usage_block,
    with_service_tier, ResponseLifecycle, ResponseLifecycleEnd, ALL_UNAVAILABLE_FRAME, RETRY_FRAME,
};
use super::super::Engine;
use super::connect::UpstreamSocket;
use message::{to_client, to_server};

mod message;

pub(super) async fn run(
    mut client: WebSocket,
    mut upstream: UpstreamSocket,
    engine: std::sync::Arc<Engine>,
    account: AccountId,
    initial_thread: Option<ThreadId>,
) {
    let attachment = initial_thread
        .as_ref()
        .and_then(|thread| ThreadAttachment::open(engine.clone(), &account, thread).ok());
    let mut turn = Turn {
        active: false,
        visible: false,
        thread: initial_thread,
        attachment,
        lease: None,
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
                if handle_server(&mut client, &engine, &account, &mut turn, message).await.is_err() {
                    break;
                }
            }
        }
    }
}

struct Turn {
    active: bool,
    visible: bool,
    thread: Option<ThreadId>,
    attachment: Option<ThreadAttachment>,
    lease: Option<StreamLease>,
    lifecycle: ResponseLifecycle,
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
            let draining = requested_thread
                .as_ref()
                .is_some_and(|thread| engine.drains_in_place(account, thread));
            if (!turn.active || changes_thread) && !draining {
                match engine.eligible_account().ok().flatten() {
                    Some(selected) if &selected != account => {
                        client
                            .send(ClientMessage::Text(RETRY_FRAME.into()))
                            .await
                            .map_err(|_| ())?;
                        return Err(());
                    }
                    None => {
                        wait(engine, &turn.thread);
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
                    turn.attachment = ThreadAttachment::open(engine.clone(), account, thread).ok();
                }
            }
            turn.thread = requested_thread;
            if turn.lease.is_none() {
                if let Some(id) = turn.thread.as_ref() {
                    turn.lease = StreamLease::open(engine.clone(), account, id).ok();
                }
            }
            turn.active = true;
            turn.visible = false;
            turn.lifecycle.reset();
            // Burn the remainder fast, for the models that offer it.
            let upgraded = draining
                .then(|| requested_model(text))
                .flatten()
                .and_then(|model| engine.fast_tier(&model))
                .and_then(|tier| with_service_tier(text, tier));
            if let Some(upgraded) = upgraded {
                return upstream
                    .send(ServerMessage::Text(upgraded.as_str().into()))
                    .await
                    .map_err(|_| ());
            }
        }
    }
    upstream.send(to_server(message)).await.map_err(|_| ())
}

async fn handle_server(
    client: &mut WebSocket,
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    message: ServerMessage,
) -> Result<(), ()> {
    if let ServerMessage::Text(text) = &message {
        if let Some(block) = websocket_usage_block(text) {
            turn.active = false;
            turn.lease.take();
            let _ = engine.block(account, block.resets_at);
            if turn.visible {
                client.send(to_client(message)).await.map_err(|_| ())?;
                if engine.eligible_account().ok().flatten().is_none() {
                    wait(engine, &turn.thread);
                }
                return Ok(());
            }
            if engine.eligible_account().ok().flatten().is_some() {
                client
                    .send(ClientMessage::Text(RETRY_FRAME.into()))
                    .await
                    .map_err(|_| ())?;
            } else {
                wait(engine, &turn.thread);
                client.send(to_client(message)).await.map_err(|_| ())?;
            }
            return Err(());
        }
        turn.visible |= model_visible_output(text);
        match turn.lifecycle.observe_json(text.as_bytes()) {
            Some(ResponseLifecycleEnd::Continue) => {
                turn.active = false;
                if let Some(mut lease) = turn.lease.take() {
                    lease.continue_after_response();
                }
            }
            Some(ResponseLifecycleEnd::Finish) => {
                turn.active = false;
                turn.lease.take();
            }
            None => {}
        }
    }
    client.send(to_client(message)).await.map_err(|_| ())
}

fn wait(engine: &Engine, thread: &Option<ThreadId>) {
    if let Some(thread) = thread {
        let _ = engine.waiting(thread);
    }
}
