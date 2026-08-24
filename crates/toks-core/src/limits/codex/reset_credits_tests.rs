use super::reset_credits::{into_domain, ResetCreditDetailsResponse};
use crate::limits::BankedResetCreditStatus;

#[test]
fn parses_optional_credit_details_without_using_the_detail_count() {
    let response = serde_json::from_value::<ResetCreditDetailsResponse>(serde_json::json!({
        "available_count": 99,
        "credits": [
            {
                "id": "credit-1",
                "reset_type": "codex_rate_limits",
                "status": "available",
                "granted_at": "2026-08-01T12:00:00Z",
                "expires_at": "2026-09-01T12:00:00Z",
                "title": "Summer reset"
            },
            {
                "id": "credit-2",
                "reset_type": "codex_rate_limits",
                "granted_at": "2026-08-02T12:00:00Z",
                "expires_at": null
            }
        ]
    }))
    .expect("documented detail response parses");

    let credits = into_domain(response);
    assert_eq!(credits.len(), 2);
    assert_eq!(credits[0].title.as_deref(), Some("Summer reset"));
    assert_eq!(credits[0].status, Some(BankedResetCreditStatus::Available));
    assert!(credits[0].expires_at.is_some());
    assert_eq!(credits[1].status, None);
    assert_eq!(credits[1].expires_at, None);
}

#[test]
fn malformed_expiry_rejects_only_the_optional_detail_response() {
    let result = serde_json::from_value::<ResetCreditDetailsResponse>(serde_json::json!({
        "available_count": 1,
        "credits": [{"expires_at": "later"}]
    }));
    assert!(result.is_err());
}
