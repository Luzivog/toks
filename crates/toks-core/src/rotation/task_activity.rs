use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{ThreadId, ThreadRequestSettings, UnixMillis};

mod projection;
mod replacement;
mod request_settings;
mod storage;
pub use storage::TaskActivityStore;

#[cfg(test)]
mod tests;

const TASK_ACTIVITY_VERSION: u8 = 1;
pub const TASK_ACTIVITY_FRESHNESS_MILLIS: i64 = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveTask {
    pub account_id: AccountId,
    #[serde(with = "request_settings")]
    pub request_settings: ThreadRequestSettings,
    pub started_at: UnixMillis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTaskRow {
    pub thread_id: ThreadId,
    pub account_id: AccountId,
    pub request_settings: ThreadRequestSettings,
    pub started_at: UnixMillis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenerationTaskActivity {
    pub task_count: u32,
    pub oldest_task_at: Option<UnixMillis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskActivity {
    version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_workers: Option<BTreeMap<u64, u64>>,
    #[serde(default)]
    workers: BTreeMap<u64, BTreeMap<u64, WorkerTaskSnapshot>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerTaskSnapshot {
    revision: u64,
    observed_at: UnixMillis,
    tasks: BTreeMap<ThreadId, ActiveTask>,
}

impl Default for TaskActivity {
    fn default() -> Self {
        Self {
            version: TASK_ACTIVITY_VERSION,
            expected_workers: None,
            workers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskActivityConflict {
    InvalidWorker { generation: u64 },
    RevisionReused { generation: u64, revision: u64 },
}

impl fmt::Display for TaskActivityConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorker { generation } => {
                write!(
                    formatter,
                    "task activity worker {generation} has an invalid identity"
                )
            }
            Self::RevisionReused {
                generation,
                revision,
            } => write!(
                formatter,
                "task activity worker {generation} reused revision {revision}"
            ),
        }
    }
}

impl std::error::Error for TaskActivityConflict {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskActivityUnavailable {
    TopologyUnknown,
    MissingWorker { generation: u64 },
    StaleWorker { generation: u64 },
    FutureWorker { generation: u64 },
    ConflictingTask { thread_id: ThreadId },
}

impl fmt::Display for TaskActivityUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TopologyUnknown => formatter.write_str("task activity topology is unknown"),
            Self::MissingWorker { generation } => {
                write!(
                    formatter,
                    "router generation {generation} has no activity snapshot"
                )
            }
            Self::StaleWorker { generation } => {
                write!(
                    formatter,
                    "router generation {generation} activity is stale"
                )
            }
            Self::FutureWorker { generation } => write!(
                formatter,
                "router generation {generation} activity has a future timestamp"
            ),
            Self::ConflictingTask { thread_id } => write!(
                formatter,
                "task {} has conflicting worker activity",
                thread_id.as_str()
            ),
        }
    }
}

impl std::error::Error for TaskActivityUnavailable {}
