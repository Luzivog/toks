use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualTestOutcome {
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualTestReceipt {
    pub requested_account: AccountId,
    pub observed_account: Option<AccountId>,
    pub thread_id: Option<ThreadId>,
    pub started_at_ms: i64,
    pub routed_at_ms: Option<i64>,
    pub completed_at_ms: i64,
    pub outcome: ManualTestOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountActivationStatus {
    pub automatic_enabled: bool,
    pub active_until_ms: Option<i64>,
    pub automatic: AutomaticTestStatus,
    pub manual: ManualTestStatus,
    pub manual_receipt: Option<ManualTestReceipt>,
}

impl Default for AccountActivationStatus {
    fn default() -> Self {
        Self {
            automatic_enabled: true,
            active_until_ms: None,
            automatic: AutomaticTestStatus::Ready,
            manual: ManualTestStatus::Ready,
            manual_receipt: None,
        }
    }
}
