use axum::extract::ws::WebSocket;
use tokio_tungstenite::tungstenite::Message as ServerMessage;

use crate::accounts::AccountId;

use super::super::super::protocol::{
    starts_response_delivery, websocket_usage_block, ResponseLifecycleEnd,
};
use super::super::super::Engine;
use super::super::connect::UpstreamSocket;
use super::{message::to_client, usage_limit, Turn};

pub(super) async fn handle(
    client: &mut WebSocket,
    upstream: &mut UpstreamSocket,
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    message: ServerMessage,
) -> Result<(), ()> {
    let mut delivers_response = false;
    if let ServerMessage::Text(text) = &message {
        if let Some(block) = websocket_usage_block(text) {
            return usage_limit::handle(client, upstream, engine, account, turn, message, block)
                .await;
        }
        delivers_response = starts_response_delivery(text);
        match turn.lifecycle.observe_json(text.as_bytes()) {
            Some(ResponseLifecycleEnd::Continue) => {
                turn.active = false;
                turn.forced_fast_request = None;
                if let Some(mut lease) = turn.lease.take() {
                    lease.continue_after_response();
                }
            }
            Some(ResponseLifecycleEnd::Finish) => {
                turn.active = false;
                turn.forced_fast_request = None;
                turn.lease.take();
            }
            None => {}
        }
    }
    client.send(to_client(message)).await.map_err(|_| ())?;
    turn.delivered |= delivers_response;
    Ok(())
}
