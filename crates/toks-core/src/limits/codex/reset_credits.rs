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
