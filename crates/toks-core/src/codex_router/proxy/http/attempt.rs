use std::collections::VecDeque;

use axum::body::{Body, Bytes};
use axum::http::header::CONTENT_TYPE;
use axum::http::{Response, StatusCode};
use futures_util::StreamExt;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::engine::{AttemptedTier, ResponseDelivery, RouteTier, UsageLimitAction};
use super::super::headers::response_headers;
use super::super::lease::StreamLease;
use super::super::protocol::{requested_model, usage_block, with_service_tier, ResponseLifecycle};
use super::super::ProxyState;
use super::{build_response, stream};

const MAX_SSE_PREFETCH_BYTES: usize = 4 * 1024 * 1024;

pub(super) enum Attempt {
    Response(Response<Body>),
    TryNext(AccountId),
    RetrySameAccountAtStandardTier,
    Failed,
}

pub(super) fn request_body(
    state: &ProxyState,
    tier: RouteTier,
    thread: &Option<ThreadId>,
    body: Bytes,
) -> (Bytes, bool) {
    let Some((_thread, text)) = thread.as_ref().zip(std::str::from_utf8(&body).ok()) else {
        return (body, false);
    };
    let upgraded = match tier {
        RouteTier::Original => None,
        RouteTier::Standard => with_service_tier(text, "default").map(|body| (body, false)),
        RouteTier::Fast => requested_model(text)
            .and_then(|model| state.engine.fast_tier_for(&model))
            .and_then(|tier| with_service_tier(text, tier))
            .map(|upgraded| {
                let forced = upgraded.as_bytes() != body.as_ref();
                (upgraded, forced)
            }),
    };
    match upgraded {
        Some((upgraded, forced)) => (upgraded.into(), forced),
        None => (body, false),
    }
}

pub(super) async fn classify_response(
    state: &ProxyState,
    response: reqwest::Response,
    account: AccountId,
    thread: &Option<ThreadId>,
    lease: Option<StreamLease>,
    forced_fast: bool,
) -> Attempt {
    let status = response.status();
    let headers = response_headers(response.headers());
    if status == StatusCode::TOO_MANY_REQUESTS {
        let body = response.bytes().await.unwrap_or_default();
        if let Some(block) = usage_block(status.as_u16(), &body) {
            drop(lease);
            let tier = if forced_fast {
                AttemptedTier::ToksForcedFast
            } else {
                AttemptedTier::Other
            };
            match state.engine.request_usage_limited(
                &account,
                thread.as_ref(),
                tier,
                ResponseDelivery::NothingDelivered,
                block.resets_at,
            ) {
                Ok(UsageLimitAction::RetrySameAccountAtStandardTier) => {
                    return Attempt::RetrySameAccountAtStandardTier;
                }
                Ok(UsageLimitAction::TryAnotherAccount) => {}
                Ok(UsageLimitAction::ForwardFailure) => {
                    return Attempt::Response(build_response(status, headers, Body::from(body)));
                }
                Err(_) => return Attempt::Failed,
            }
            let eligible = thread.as_ref().map_or_else(
                || state.engine.eligible_account(),
                |thread| state.engine.eligible_account_for_thread(thread),
            );
            if eligible.ok().flatten().is_some() {
                return Attempt::TryNext(account);
            }
            if let Some(thread) = thread {
                let _ = state.engine.waiting(thread);
            }
        }
        return Attempt::Response(build_response(status, headers, Body::from(body)));
    }
    let tier = if forced_fast {
        AttemptedTier::ToksForcedFast
    } else {
        AttemptedTier::Other
    };
    let is_sse = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    let mut upstream = response.bytes_stream().boxed();
    let mut lifecycle = ResponseLifecycle::default();
    let mut prefetched = Vec::new();
    let mut end = None;
    let mut usage = None;
    let mut usage_after_prior_event = false;
    let mut initial_error = None;
    while let Some(chunk) = upstream.next().await {
        match chunk {
            Ok(bytes) => {
                prefetched.extend_from_slice(&bytes);
                let observation = lifecycle.observe_sse(&bytes);
                end = observation.end.or(end);
                usage = observation.usage.or(usage);
                usage_after_prior_event |= observation.usage_after_prior_event;
                if !is_sse || observation.events > 0 || prefetched.len() >= MAX_SSE_PREFETCH_BYTES {
                    break;
                }
            }
            Err(error) => {
                initial_error = Some(error);
                break;
            }
        }
    }
    if let Some(block) = usage {
        drop(lease);
        let delivery = if usage_after_prior_event {
            ResponseDelivery::Delivered
        } else {
            ResponseDelivery::NothingDelivered
        };
        return match state.engine.request_usage_limited(
            &account,
            thread.as_ref(),
            tier,
            delivery,
            block.resets_at,
        ) {
            Ok(UsageLimitAction::RetrySameAccountAtStandardTier) => {
                Attempt::RetrySameAccountAtStandardTier
            }
            Ok(UsageLimitAction::TryAnotherAccount) => Attempt::TryNext(account),
            Ok(UsageLimitAction::ForwardFailure) => {
                Attempt::Response(build_response(status, headers, Body::from(prefetched)))
            }
            Err(_) => Attempt::Failed,
        };
    }
    let mut initial = VecDeque::new();
    if !prefetched.is_empty() {
        initial.push_back((Ok(Bytes::from(prefetched)), end));
    }
    if let Some(error) = initial_error {
        initial.push_back((Err(error), None));
    }
    if initial.is_empty() {
        return Attempt::Response(build_response(status, headers, Body::empty()));
    }
    let body = stream::body(
        initial,
        upstream,
        lease,
        lifecycle,
        stream::UsageContext {
            engine: state.engine.clone(),
            account,
            thread: thread.clone(),
            tier,
        },
    );
    Attempt::Response(build_response(status, headers, body))
}
