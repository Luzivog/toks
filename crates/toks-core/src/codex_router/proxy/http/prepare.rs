use axum::body::Bytes;

use crate::rotation::{ThreadId, UsageLimitTier, UsageLimitTierOrigin};

use super::request_body::{CodexHttpBody, RewriteError};
use crate::codex_router::proxy::engine::RouteTier;
use crate::codex_router::proxy::protocol::{requested_model, requested_service_tier};
use crate::codex_router::proxy::ProxyState;

pub(super) struct PreparedRequest {
    pub wire: Bytes,
    pub forced_fast: bool,
    pub model: Option<String>,
    pub tier: UsageLimitTier,
}

pub(super) async fn request_body(
    state: &ProxyState,
    route: RouteTier,
    thread: &Option<ThreadId>,
    body: &CodexHttpBody,
    is_responses: bool,
    max_wire_bytes: usize,
) -> Result<PreparedRequest, RewriteError> {
    let text = body.text();
    let model = text.and_then(requested_model);
    let service_tier = text.and_then(requested_service_tier);
    let original_tier = UsageLimitTier::client(service_tier.as_deref());
    let Some((_thread, text)) = thread.as_ref().zip(text) else {
        return Ok(prepared(body.wire(), false, model, original_tier));
    };
    if !is_responses {
        return Ok(prepared(body.wire(), false, model, original_tier));
    }
    let requested = match route {
        RouteTier::Original => None,
        RouteTier::Standard => Some(("default", false, UsageLimitTierOrigin::ToksStandardFallback)),
        RouteTier::Fast => requested_model(text)
            .and_then(|model| state.engine.fast_tier_for(&model))
            .map(|tier| (tier, true, UsageLimitTierOrigin::ToksForcedFast)),
    };
    let Some((requested_tier, is_fast, changed_origin)) = requested else {
        return Ok(prepared(body.wire(), false, model, original_tier));
    };
    let rewritten = body
        .with_service_tier(requested_tier, is_fast, max_wire_bytes)
        .await?;
    let tier = recorded_tier(body.decoded(), &rewritten.forwarded, changed_origin);
    Ok(prepared(rewritten.wire, rewritten.forced_fast, model, tier))
}

pub(super) fn recorded_tier(
    original: &[u8],
    forwarded: &str,
    changed_origin: UsageLimitTierOrigin,
) -> UsageLimitTier {
    let forwarded_tier = requested_service_tier(forwarded);
    let changed = forwarded.as_bytes() != original;
    let origin = match changed_origin {
        UsageLimitTierOrigin::ToksStandardFallback
            if !changed && forwarded_tier.as_deref().is_some_and(is_fast_tier) =>
        {
            UsageLimitTierOrigin::Client
        }
        UsageLimitTierOrigin::ToksStandardFallback => UsageLimitTierOrigin::ToksStandardFallback,
        UsageLimitTierOrigin::ToksForcedFast if changed => UsageLimitTierOrigin::ToksForcedFast,
        _ if forwarded_tier.is_some() => UsageLimitTierOrigin::Client,
        _ => UsageLimitTierOrigin::Unspecified,
    };
    UsageLimitTier::new(forwarded_tier.as_deref(), origin)
}

fn is_fast_tier(tier: &str) -> bool {
    matches!(tier, "fast" | "priority" | "ultrafast")
}

fn prepared(
    wire: Bytes,
    forced_fast: bool,
    model: Option<String>,
    tier: UsageLimitTier,
) -> PreparedRequest {
    PreparedRequest {
        wire,
        forced_fast,
        model,
        tier,
    }
}
