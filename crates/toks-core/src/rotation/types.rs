use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

mod usage_limit;
pub use usage_limit::{
    UsageLimitClassification, UsageLimitEvidence, UsageLimitIncident, UsageLimitPhase,
    UsageLimitTier, UsageLimitTierOrigin,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixMillis(i64);

impl UnixMillis {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockWindow {
    until: UnixMillis,
    reset_known: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FastLimitDisposition {
    RetryingStandard,
    NextRequestUsesStandard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FastLimitOutcome {
    UseStandard,
    AlreadyBlocked,
}

impl BlockWindow {
    pub const fn known(until: UnixMillis) -> Self {
        Self {
            until,
            reset_known: true,
        }
    }

    pub const fn reprobe_at(until: UnixMillis) -> Self {
        Self {
            until,
            reset_known: false,
        }
    }

    pub const fn until(self) -> UnixMillis {
        self.until
    }

    pub const fn reset_known(self) -> bool {
        self.reset_known
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(String);

impl ThreadId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RotationEventKind {
    Routed {
        thread_id: ThreadId,
        account_id: AccountId,
    },
    Rotated {
        thread_id: ThreadId,
        from: AccountId,
        to: AccountId,
    },
    Blocked {
        account_id: AccountId,
        until: UnixMillis,
    },
    ThreadBlocked {
        thread_id: ThreadId,
        account_id: AccountId,
        until: UnixMillis,
    },
    FastFallback {
        thread_id: ThreadId,
        account_id: AccountId,
    },
    FastUnavailable {
        thread_id: ThreadId,
        account_id: AccountId,
    },
    Draining {
        account_id: AccountId,
    },
    AuthNeeded {
        account_id: AccountId,
    },
    Waiting {
        thread_id: ThreadId,
    },
    Resumed {
        thread_id: ThreadId,
        account_id: AccountId,
    },
    UsageLimited {
        account_id: AccountId,
        incident: UsageLimitIncident,
    },
    RouterFailure,
}

impl RotationEventKind {
    pub(super) const fn is_incident(&self) -> bool {
        matches!(self, Self::UsageLimited { .. } | Self::RouterFailure)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationEvent {
    pub at: UnixMillis,
    #[serde(flatten)]
    pub event: RotationEventKind,
}
