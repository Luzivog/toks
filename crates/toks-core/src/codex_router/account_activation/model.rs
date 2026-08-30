use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::accounts::{AccountId, CredentialProfileId};
use crate::rotation::ThreadId;

use super::owner::ProcessOwner;
use super::status::ManualTestReceipt;

pub(super) const DOCUMENT_VERSION: u8 = 1;
pub(super) const TASK_TIMEOUT_MS: i64 = 3 * 60 * 1_000;
pub(super) const PROVISIONAL_WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
pub(super) const RETRY_DELAYS_MS: [i64; 3] = [60_000, 300_000, 900_000];

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Document {
    pub(super) version: u8,
    #[serde(default)]
    pub(super) disabled: BTreeSet<AccountId>,
    #[serde(default)]
    pub(super) accounts: BTreeMap<AccountId, AccountState>,
}

impl Document {
    pub(super) fn new() -> Self {
        Self {
            version: DOCUMENT_VERSION,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountState {
    pub(super) active_until_ms: Option<i64>,
    pub(super) automatic: Option<Job>,
    pub(super) manual: Option<Job>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) manual_receipt: Option<ManualTestReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Job {
    pub(super) id: String,
    pub(super) kind: JobKind,
    pub(super) launches: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) owner: Option<ProcessOwner>,
    pub(super) phase: JobPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) manual_route: Option<ManualRoute>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub(super) enum ManualRoute {
    Bound {
        thread_id: ThreadId,
        bound_at_ms: i64,
    },
    Routed {
        thread_id: ThreadId,
        bound_at_ms: i64,
        routed_at_ms: i64,
        observed_account: AccountId,
    },
}

impl ManualRoute {
    pub(super) fn thread_id(&self) -> &ThreadId {
        match self {
            Self::Bound { thread_id, .. } | Self::Routed { thread_id, .. } => thread_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(super) enum JobKind {
    Automatic {
        predecessor_active_until_ms: Option<i64>,
    },
    Manual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub(super) enum JobPhase {
    Pending {
        not_before_ms: i64,
    },
    Running {
        started_at_ms: i64,
        profile_id: CredentialProfileId,
    },
    Checking {
        failed_at_ms: i64,
        not_before_ms: i64,
    },
    Succeeded {
        completed_at_ms: i64,
    },
    NeedsAttention {
        failed_at_ms: i64,
        reason: FailureReason,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FailureReason {
    Interrupted,
    ModelUnavailable,
    ProfileUnavailable,
    SpawnFailed,
    TimedOut,
    Unsuccessful,
    RouteUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LaunchKind {
    Automatic,
    Manual,
}

#[derive(Clone, Debug)]
pub(crate) struct Launch {
    pub(super) id: String,
    pub(super) account: AccountId,
    pub(super) profile_id: CredentialProfileId,
    pub(super) kind: LaunchKind,
}
