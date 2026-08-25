use crate::codex_router::proxy::engine::RouteTier;
use crate::codex_router::proxy::lease::StreamLease;
use crate::codex_router::proxy::protocol::{
    requested_settings, rewrite_request, RequestEnvelope, RewrittenRequest,
};
use crate::codex_router::proxy::Engine;
use crate::rotation::{ThreadOverride, UsageLimitTierOrigin};

pub(super) struct PreparedRequest {
    pub(super) forwarded: String,
    pub(super) fallback: Option<String>,
    pub(super) origin: UsageLimitTierOrigin,
}

pub(super) fn prepare(
    engine: &Engine,
    lease: Option<&StreamLease>,
    payload: &str,
) -> PreparedRequest {
    let route = lease.map_or(RouteTier::Original, StreamLease::tier);
    let request_override = lease.and_then(StreamLease::request_override);
    let observed = requested_settings(payload);
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
                .and_then(|model| engine.fast_tier_for(model))
                .map(|tier| (tier, UsageLimitTierOrigin::ToksForcedFast)),
        }
    };
    let rewritten = rewrite_request(
        payload,
        RequestEnvelope::ResponseCreate,
        request_override,
        automatic.map(|(tier, _)| tier),
    )
    .unwrap_or_else(|| RewrittenRequest {
        payload: payload.to_owned(),
        automatic_tier_applied: false,
    });
    let forwarded_tier = requested_settings(&rewritten.payload).service_tier;
    let origin = match automatic.map(|(_, origin)| origin) {
        Some(UsageLimitTierOrigin::ToksStandardFallback)
            if !forwarded_tier.as_deref().is_some_and(is_fast_tier) =>
        {
            UsageLimitTierOrigin::ToksStandardFallback
        }
        Some(UsageLimitTierOrigin::ToksForcedFast) if rewritten.automatic_tier_applied => {
            UsageLimitTierOrigin::ToksForcedFast
        }
        _ => UsageLimitTierOrigin::Client,
    };
    let fallback = (origin == UsageLimitTierOrigin::ToksForcedFast).then(|| {
        rewrite_request(
            payload,
            RequestEnvelope::ResponseCreate,
            request_override,
            Some("default"),
        )
        .map_or_else(|| payload.to_owned(), |fallback| fallback.payload)
    });
    PreparedRequest {
        forwarded: rewritten.payload,
        fallback,
        origin,
    }
}

fn is_fast_tier(tier: &str) -> bool {
    matches!(tier, "fast" | "priority" | "ultrafast")
}
