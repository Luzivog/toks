use std::collections::VecDeque;
use std::io;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use futures_util::stream::BoxStream;
use futures_util::StreamExt;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::engine::{AttemptedTier, Engine, ResponseDelivery};
use super::super::lease::StreamLease;
use super::super::protocol::{ResponseLifecycle, ResponseLifecycleEnd};

type UpstreamBody = BoxStream<'static, Result<Bytes, reqwest::Error>>;

pub(super) struct UsageContext {
    pub engine: Arc<Engine>,
    pub account: AccountId,
    pub thread: Option<ThreadId>,
    pub tier: AttemptedTier,
}

pub(super) fn body(
    initial: VecDeque<(Result<Bytes, reqwest::Error>, Option<ResponseLifecycleEnd>)>,
    rest: UpstreamBody,
    lease: Option<StreamLease>,
    lifecycle: ResponseLifecycle,
    usage: UsageContext,
) -> Body {
    let state = StreamState {
        initial,
        rest,
        lease,
        lifecycle,
        usage,
    };
    Body::from_stream(futures_util::stream::unfold(state, next_chunk))
}

struct StreamState {
    initial: VecDeque<(Result<Bytes, reqwest::Error>, Option<ResponseLifecycleEnd>)>,
    rest: UpstreamBody,
    lease: Option<StreamLease>,
    lifecycle: ResponseLifecycle,
    usage: UsageContext,
}

async fn next_chunk(mut state: StreamState) -> Option<(Result<Bytes, io::Error>, StreamState)> {
    let (chunk, end) = match state.initial.pop_front() {
        Some(initial) => initial,
        None => {
            let chunk = state.rest.next().await?;
            let end = match &chunk {
                Ok(bytes) => {
                    let observation = state.lifecycle.observe_sse(bytes);
                    if let Some(block) = observation.usage {
                        if let Err(error) = state.usage.engine.request_usage_limited(
                            &state.usage.account,
                            state.usage.thread.as_ref(),
                            state.usage.tier,
                            ResponseDelivery::Delivered,
                            block.resets_at,
                        ) {
                            return Some((Err(io::Error::other(error)), state));
                        }
                    }
                    observation.end
                }
                Err(_) => None,
            };
            (chunk, end)
        }
    };
    match end {
        Some(ResponseLifecycleEnd::Continue) => {
            if let Some(mut lease) = state.lease.take() {
                lease.continue_after_response();
            }
        }
        Some(ResponseLifecycleEnd::Finish) => {
            state.lease.take();
        }
        None => {}
    }
    let chunk = chunk.map_err(io::Error::other);
    Some((chunk, state))
}
