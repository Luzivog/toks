use super::connect::UpstreamSocket;
use crate::accounts::AccountId;
use crate::codex_router::proxy::lease::ThreadAttachment;
use crate::codex_router::proxy::protocol::ResponseLifecycle;
use crate::codex_router::proxy::Engine;
use crate::rotation::ThreadId;
use crate::rotation::UsageLimitTier;
use axum::extract::ws::WebSocket;
use futures_util::StreamExt;
mod admission;
mod client;
mod message;
mod server;
mod turn;
mod usage_limit;
pub(super) use admission::reject_thread;
use turn::Turn;
pub(super) async fn run(
    mut client: WebSocket,
    mut upstream: UpstreamSocket,
    engine: std::sync::Arc<Engine>,
    account: AccountId,
    header_thread: Option<ThreadId>,
    initial_thread: Option<ThreadId>,
    resume_attempt: Option<String>,
) {
    let attachment = match initial_thread.as_ref() {
        Some(thread) => match ThreadAttachment::open(
            engine.clone(),
            &account,
            thread,
            resume_attempt.as_deref(),
        ) {
            Ok(Some(attachment)) => Some(attachment),
            Ok(None) | Err(_) => return,
        },
        None => None,
    };
    let mut turn = Turn {
        active: false,
        delivered: false,
        header_thread,
        thread: initial_thread,
        attachment,
        lease: None,
        forced_fast_request: None,
        model: None,
        request_tier: UsageLimitTier::unspecified(),
        lifecycle: ResponseLifecycle::default(),
        resume_attempt,
    };
    loop {
        tokio::select! {
            client_message = client.next() => {
                let Some(Ok(message)) = client_message else { break };
                if client::handle(&mut client, &mut upstream, &engine, &account,
                    &mut turn, message).await.is_none() {
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
                ).await.is_none() {
                    break;
                }
            }
        }
    }
}
