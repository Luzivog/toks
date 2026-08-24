use crate::rotation::UsageLimitTierOrigin;

use super::super::super::super::engine::RouteTier;
use super::super::super::super::lease::StreamLease;
use super::super::super::super::protocol::{
    requested_model, requested_service_tier, with_service_tier,
};
use super::super::super::super::Engine;
use super::super::Turn;

pub(super) fn upgraded_request(
    engine: &Engine,
    turn: &Turn,
    text: &str,
) -> Option<(String, Option<String>, UsageLimitTierOrigin)> {
    match turn
        .lease
        .as_ref()
        .map_or(RouteTier::Original, StreamLease::tier)
    {
        RouteTier::Original => None,
        RouteTier::Standard => with_service_tier(text, "default").map(|body| {
            let preserved_fast = body == text
                && requested_service_tier(text)
                    .as_deref()
                    .is_some_and(|tier| matches!(tier, "fast" | "priority" | "ultrafast"));
            let origin = if preserved_fast {
                UsageLimitTierOrigin::Client
            } else {
                UsageLimitTierOrigin::ToksStandardFallback
            };
            (body, None, origin)
        }),
        RouteTier::Fast => requested_model(text)
            .and_then(|model| engine.fast_tier_for(&model))
            .and_then(|tier| with_service_tier(text, tier))
            .map(|body| {
                let fallback = (body != text)
                    .then(|| with_service_tier(text, "default").unwrap_or_else(|| text.to_owned()));
                let origin = if fallback.is_some() {
                    UsageLimitTierOrigin::ToksForcedFast
                } else {
                    UsageLimitTierOrigin::Client
                };
                (body, fallback, origin)
            }),
    }
}
