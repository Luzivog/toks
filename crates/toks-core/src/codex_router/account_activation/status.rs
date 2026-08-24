use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualTestStatus {
    Ready,
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomaticTestStatus {
    Ready,
    Pending,
    Running,
    Succeeded,
    NeedsAttention,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountActivationStatus {
    pub automatic_enabled: bool,
    pub active_until_ms: Option<i64>,
    pub automatic: AutomaticTestStatus,
    pub manual: ManualTestStatus,
}

impl Default for AccountActivationStatus {
    fn default() -> Self {
        Self {
            automatic_enabled: true,
            active_until_ms: None,
            automatic: AutomaticTestStatus::Ready,
            manual: ManualTestStatus::Ready,
        }
    }
}
