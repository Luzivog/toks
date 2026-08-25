use super::prepare::recorded_tier;
use crate::codex_router::proxy::protocol::with_service_tier;
use crate::rotation::UsageLimitTierOrigin;

#[test]
fn incident_observability_records_the_actual_client_fast_tier_on_a_standard_route() {
    let original = r#"{"type":"response.create","service_tier":"priority"}"#;
    let forwarded = with_service_tier(original, "default").unwrap();
    assert_eq!(forwarded, original);

    let tier = recorded_tier(
        &forwarded,
        Some(UsageLimitTierOrigin::ToksStandardFallback),
        false,
    );
    assert_eq!(tier.effective(), Some("priority"));
    assert_eq!(tier.origin(), UsageLimitTierOrigin::Client);

    let default = r#"{"type":"response.create","service_tier":"default"}"#;
    let tier = recorded_tier(
        default,
        Some(UsageLimitTierOrigin::ToksStandardFallback),
        false,
    );
    assert_eq!(tier.effective(), Some("default"));
    assert_eq!(tier.origin(), UsageLimitTierOrigin::ToksStandardFallback);
}
