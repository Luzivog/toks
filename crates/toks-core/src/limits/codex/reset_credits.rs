use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::limits::{BankedResetCredit, BankedResetCreditStatus};

/// Wire response from Codex's optional reset-credit detail endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct ResetCreditDetailsResponse {
    #[serde(default)]
    credits: Vec<ResetCreditDetails>,
    // The usage response owns the displayed count. This field only verifies
    // that the detail endpoint returned its documented response shape.
    #[serde(rename = "available_count")]
    _available_count: i64,
}

#[derive(Debug, Deserialize)]
struct ResetCreditDetails {
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    status: Option<BankedResetCreditStatus>,
}

pub(crate) fn into_domain(response: ResetCreditDetailsResponse) -> Vec<BankedResetCredit> {
    response
        .credits
        .into_iter()
        .map(|credit| BankedResetCredit {
            expires_at: credit.expires_at,
            title: credit.title,
            status: credit.status,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{into_domain, BankedResetCreditStatus, ResetCreditDetailsResponse};

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
}
