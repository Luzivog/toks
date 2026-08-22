use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Provider detail for one banked Codex reset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankedResetCredit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<BankedResetCreditStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankedResetCreditStatus {
    Available,
    Redeeming,
    Redeemed,
    #[serde(other)]
    Unknown,
}

impl BankedResetCreditStatus {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::Redeeming => "Redeeming",
            Self::Redeemed => "Redeemed",
            Self::Unknown => "Unknown",
        }
    }
}
