use axum::extract::ws::{Message as ClientMessage, WebSocket};
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message as ServerMessage;

use crate::accounts::AccountId;

use super::super::connect::UpstreamSocket;
use super::{message::to_client, Turn};
use crate::codex_router::proxy::protocol::{UsageBlock, RETRY_FRAME};
use crate::codex_router::proxy::{
    engine::{AttemptedTier, ResponseDelivery, UsageLimitAction},
    Engine,
};

pub(super) async fn handle(
    client: &mut WebSocket,
    upstream: &mut UpstreamSocket,
    engine: &std::sync::Arc<Engine>,
    account: &AccountId,
    turn: &mut Turn,
    message: ServerMessage,
    block: UsageBlock,
) -> Result<(), ()> {
    let forced_request = turn.forced_fast_request.take();
    let tier = if forced_request.is_some() {
        AttemptedTier::ToksForcedFast
    } else {
        AttemptedTier::Other
    };
    let delivery = if turn.delivered {
        ResponseDelivery::Delivered
    } else {
        ResponseDelivery::NothingDelivered
    };
    let action = match engine.request_usage_limited(
        account,
        turn.thread.as_ref(),
        tier,
        delivery,
        block.resets_at,
    ) {
        Ok(action) => action,
        Err(_) => {
            let _ = client.send(to_client(message)).await;
            return Err(());
        }
    };
    if action == UsageLimitAction::RetrySameAccountAtStandardTier {
        let original = forced_request.expect("forced Fast retry retains its original request");
        turn.lifecycle.reset();
        return upstream
            .send(ServerMessage::Text(original.into()))
            .await
            .map_err(|_| ());
    }
    turn.active = false;
    turn.lease.take();
    if action == UsageLimitAction::ForwardFailure {
        client.send(to_client(message)).await.map_err(|_| ())?;
        let eligible = turn.thread.as_ref().map_or_else(
            || engine.eligible_account(),
            |thread| engine.eligible_account_for_thread(thread),
        );
        if eligible.ok().flatten().is_none() {
            wait(engine, &turn.thread);
        }
        return Ok(());
    }
    let eligible = turn.thread.as_ref().map_or_else(
        || engine.eligible_account(),
        |thread| engine.eligible_account_for_thread(thread),
    );
    if eligible.ok().flatten().is_some() {
        client
            .send(ClientMessage::Text(RETRY_FRAME.into()))
            .await
            .map_err(|_| ())?;
    } else {
        wait(engine, &turn.thread);
        client.send(to_client(message)).await.map_err(|_| ())?;
    }
    Err(())
}

pub(super) fn wait(engine: &Engine, thread: &Option<crate::rotation::ThreadId>) {
    if let Some(thread) = thread {
        let _ = engine.waiting(thread);
    }
}
