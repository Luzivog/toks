use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A provider-reported usage allowance relative to its baseline plan.
///
/// Only multipliers the provider explicitly reports are represented. A plan
/// name such as `max` or `pro` is deliberately not enough to infer one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanMultiplier {
    #[serde(rename = "5")]
    Five,
    #[serde(rename = "20")]
    Twenty,
}

impl PlanMultiplier {
    pub fn value(self) -> u8 {
        match self {
            Self::Five => 5,
            Self::Twenty => 20,
        }
    }

    pub(crate) fn from_explicit_metadata(root: &Value) -> Option<Self> {
        const POINTERS: &[&str] = &[
            "/rate_limit_tier",
            "/rateLimitTier",
            "/usage_multiplier",
            "/usageMultiplier",
            "/rate_limit_multiplier",
            "/rateLimitMultiplier",
            "/organization_rate_limit_tier",
            "/organizationRateLimitTier",
            "/oauthAccount/organizationRateLimitTier",
            "/claudeAiOauth/rateLimitTier",
        ];
        POINTERS
            .iter()
            .find_map(|pointer| root.pointer(pointer).and_then(Self::from_value))
    }

    /// Codex's raw product SKUs distinguish the two current Pro allowances.
    /// Unknown SKUs intentionally remain unqualified.
    pub(crate) fn from_codex_plan_type(plan_type: &str) -> Option<Self> {
        match plan_type.to_ascii_lowercase().as_str() {
            "prolite" => Some(Self::Five),
            "pro" => Some(Self::Twenty),
            _ => None,
        }
    }

    fn from_value(value: &Value) -> Option<Self> {
        if let Some(number) = value.as_u64() {
            return Self::from_number(number);
        }
        value.as_str().and_then(|tier| {
            tier.split(|character: char| !character.is_ascii_alphanumeric())
                .find_map(|part| match part.to_ascii_lowercase().as_str() {
                    "5" | "5x" => Some(Self::Five),
                    "20" | "20x" => Some(Self::Twenty),
                    _ => None,
                })
        })
    }

    fn from_number(number: u64) -> Option<Self> {
        match number {
            5 => Some(Self::Five),
            20 => Some(Self::Twenty),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct PlanDetails {
    pub(crate) name: Option<String>,
    pub(crate) multiplier: Option<PlanMultiplier>,
}

pub(crate) fn read_claude_plan(config_dir: &Path) -> PlanDetails {
    let Some(value) = std::fs::read_to_string(config_dir.join(".credentials.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return PlanDetails::default();
    };
    PlanDetails {
        name: value
            .pointer("/claudeAiOauth/subscriptionType")
            .and_then(Value::as_str)
            .map(str::to_string),
        multiplier: PlanMultiplier::from_explicit_metadata(&value),
    }
}
