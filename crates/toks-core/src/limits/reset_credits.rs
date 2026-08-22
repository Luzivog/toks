use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankedResetAttempt(String);

impl BankedResetAttempt {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub(crate) fn request_id(&self) -> &str {
        &self.0
    }
}

impl Default for BankedResetAttempt {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankedResetOutcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

impl BankedResetOutcome {
    pub fn used_credit(self) -> bool {
        matches!(self, Self::Reset | Self::AlreadyRedeemed)
    }
}

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
