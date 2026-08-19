use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotFreshness {
    Live,
    Cached,
    ProviderCache,
    Loading,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitIssueKind {
    Authentication,
    RateLimited,
    Network,
    InvalidResponse,
    Storage,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitIssue {
    pub kind: LimitIssueKind,
    pub message: String,
    pub attempted_at: DateTime<Utc>,
    pub retry_at: Option<DateTime<Utc>>,
}

impl LimitIssue {
    pub(crate) fn new(kind: LimitIssueKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            attempted_at: Utc::now(),
            retry_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotStatus {
    pub freshness: SnapshotFreshness,
    pub last_attempted_at: Option<DateTime<Utc>>,
    pub issue: Option<LimitIssue>,
}

impl SnapshotStatus {
    pub(crate) fn at(freshness: SnapshotFreshness) -> Self {
        Self {
            freshness,
            ..Self::default()
        }
    }

    pub(crate) fn failed(freshness: SnapshotFreshness, issue: LimitIssue) -> Self {
        Self {
            freshness,
            last_attempted_at: Some(issue.attempted_at),
            issue: Some(issue),
        }
    }
}
