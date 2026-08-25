use axum::body::Bytes;

use crate::rotation::{ThreadId, ThreadOverride, UsageLimitTier, UsageLimitTierOrigin};

use super::request_body::{CodexHttpBody, RewriteError};
use crate::codex_router::proxy::engine::RouteTier;
use crate::codex_router::proxy::protocol::{requested_service_tier, requested_settings};
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
    request_override: Option<&ThreadOverride>,
    thread: &Option<ThreadId>,
    body: &CodexHttpBody,
    is_responses: bool,
    max_wire_bytes: usize,
) -> Result<PreparedRequest, RewriteError> {
    let text = body.text();
    let observed = text.map(requested_settings).unwrap_or_default();
    let original_tier = UsageLimitTier::client(observed.service_tier.as_deref());
    if thread.is_none() || text.is_none() {
        return Ok(prepared(body.wire(), false, observed.model, original_tier));
    }
    if !is_responses {
        return Ok(prepared(body.wire(), false, observed.model, original_tier));
    }
    let effective_model = request_override
        .and_then(ThreadOverride::model)
        .or(observed.model.as_deref());
    let automatic = if request_override
        .and_then(ThreadOverride::service_tier)
        .is_some()
    {
        None
    } else {
        match route {
            RouteTier::Original => None,
            RouteTier::Standard => Some(("default", UsageLimitTierOrigin::ToksStandardFallback)),
            RouteTier::Fast => effective_model
                .and_then(|model| state.engine.fast_tier_for(model))
                .map(|tier| (tier, UsageLimitTierOrigin::ToksForcedFast)),
        }
    };
    let rewritten = body
        .rewrite_request(
            request_override,
            automatic.map(|(tier, _)| tier),
            max_wire_bytes,
        )
        .await?;
    let forwarded = requested_settings(&rewritten.forwarded);
    let forced_fast = rewritten.automatic_tier_applied
        && automatic.is_some_and(|(_, origin)| origin == UsageLimitTierOrigin::ToksForcedFast);
    let tier = recorded_tier(
        &rewritten.forwarded,
        automatic.map(|(_, origin)| origin),
        rewritten.automatic_tier_applied,
    );
    Ok(prepared(rewritten.wire, forced_fast, forwarded.model, tier))
}

pub(super) fn recorded_tier(
    forwarded: &str,
    automatic_origin: Option<UsageLimitTierOrigin>,
    automatic_tier_applied: bool,
) -> UsageLimitTier {
    let forwarded_tier = requested_service_tier(forwarded);
    let origin = match automatic_origin {
        Some(UsageLimitTierOrigin::ToksStandardFallback)
            if !forwarded_tier.as_deref().is_some_and(is_fast_tier) =>
        {
            UsageLimitTierOrigin::ToksStandardFallback
        }
        Some(UsageLimitTierOrigin::ToksForcedFast) if automatic_tier_applied => {
            UsageLimitTierOrigin::ToksForcedFast
        }
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
