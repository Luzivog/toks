use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

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
    RouterFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationEvent {
    pub at: UnixMillis,
    #[serde(flatten)]
    pub event: RotationEventKind,
}
