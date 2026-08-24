use axum::body::Bytes;

use crate::rotation::{ThreadId, UsageLimitTier, UsageLimitTierOrigin};

use super::super::engine::RouteTier;
use super::super::protocol::{requested_model, requested_service_tier};
use super::super::ProxyState;
use super::request_body::CodexHttpBody;

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
) -> Result<PreparedRequest, ()> {
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
    let (wire, forced_fast, forwarded) = body
        .with_service_tier(requested_tier, is_fast, max_wire_bytes)
        .await?;
    let tier = recorded_tier(body.decoded(), &forwarded, changed_origin);
    Ok(prepared(wire, forced_fast, model, tier))
}

fn recorded_tier(
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

#[cfg(test)]
mod tests {
    use super::{recorded_tier, UsageLimitTierOrigin};
    use crate::codex_router::proxy::protocol::with_service_tier;

    #[test]
    fn incident_observability_records_the_actual_client_fast_tier_on_a_standard_route() {
        let original = r#"{"type":"response.create","service_tier":"priority"}"#;
        let forwarded = with_service_tier(original, "default").unwrap();
        assert_eq!(forwarded, original);

        let tier = recorded_tier(
            original.as_bytes(),
            &forwarded,
            UsageLimitTierOrigin::ToksStandardFallback,
        );
        assert_eq!(tier.effective(), Some("priority"));
        assert_eq!(tier.origin(), UsageLimitTierOrigin::Client);

        let default = r#"{"type":"response.create","service_tier":"default"}"#;
        let tier = recorded_tier(
            default.as_bytes(),
            default,
            UsageLimitTierOrigin::ToksStandardFallback,
        );
        assert_eq!(tier.effective(), Some("default"));
        assert_eq!(tier.origin(), UsageLimitTierOrigin::ToksStandardFallback);
    }
}
