use axum::extract::ws::{CloseFrame as ClientClose, Message as ClientMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{
    protocol::frame::CloseFrame as ServerClose, Message as ServerMessage,
};

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::lease::StreamLease;
use super::super::protocol::{
    is_response_create, model_visible_output, response_terminal, thread_id, websocket_usage_block,
    ALL_UNAVAILABLE_FRAME, RETRY_FRAME,
};
use super::super::Engine;
use super::connect::UpstreamSocket;

pub(super) async fn run(
    mut client: WebSocket,
    mut upstream: UpstreamSocket,
    engine: std::sync::Arc<Engine>,
    account: AccountId,
    initial_thread: Option<ThreadId>,
) {
    let mut turn = Turn {
        active: false,
        visible: false,
        thread: initial_thread,
        lease: None,
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
    lease: Option<StreamLease>,
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
            turn.thread = thread_id(text.as_bytes()).or_else(|| turn.thread.clone());
            if !turn.active {
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
            if turn.lease.is_none() {
                if let Some(id) = turn.thread.as_ref() {
                    turn.lease = StreamLease::open(engine.clone(), account, id).ok();
                }
            }
            turn.active = true;
            turn.visible = false;
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
            let _ = engine.block(account, block.resets_at);
            turn.active = false;
            turn.lease.take();
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
        if response_terminal(text) {
            turn.active = false;
            turn.lease.take();
        }
    }
    client.send(to_client(message)).await.map_err(|_| ())
}

fn wait(engine: &Engine, thread: &Option<ThreadId>) {
    if let Some(thread) = thread {
        let _ = engine.waiting(thread);
    }
}

fn to_server(message: ClientMessage) -> ServerMessage {
    match message {
        ClientMessage::Text(value) => ServerMessage::Text(value.as_str().into()),
        ClientMessage::Binary(value) => ServerMessage::Binary(value),
        ClientMessage::Ping(value) => ServerMessage::Ping(value),
        ClientMessage::Pong(value) => ServerMessage::Pong(value),
        ClientMessage::Close(frame) => ServerMessage::Close(frame.map(|frame| ServerClose {
            code: frame.code.into(),
            reason: frame.reason.as_str().into(),
        })),
    }
}

fn to_client(message: ServerMessage) -> ClientMessage {
    match message {
        ServerMessage::Text(value) => ClientMessage::Text(value.as_str().into()),
        ServerMessage::Binary(value) => ClientMessage::Binary(value),
        ServerMessage::Ping(value) => ClientMessage::Ping(value),
        ServerMessage::Pong(value) => ClientMessage::Pong(value),
        ServerMessage::Close(frame) => ClientMessage::Close(frame.map(|frame| ClientClose {
            code: frame.code.into(),
            reason: frame.reason.as_str().into(),
        })),
        ServerMessage::Frame(_) => ClientMessage::Close(None),
    }
}
