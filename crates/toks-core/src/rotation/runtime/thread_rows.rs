use std::collections::btree_map::Entry;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, UnixMillis};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

impl ThreadRequestSettings {
    pub(super) fn is_empty(&self) -> bool {
        self.model.is_none() && self.reasoning_effort.is_none() && self.service_tier.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    Streaming { stream_count: u32 },
    ReservationPending,
    AwaitingFollowUp,
    AttachedIdle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRow {
    pub thread_id: ThreadId,
    pub account_id: Option<AccountId>,
    pub status: ThreadStatus,
    pub started_at: Option<UnixMillis>,
    pub last_activity_at: Option<UnixMillis>,
    pub request_settings: ThreadRequestSettings,
}

impl RotationRuntime {
    pub fn thread_rows(&self) -> Vec<ThreadRow> {
        let mut rows = self
            .active_threads
            .iter()
            .map(|(thread_id, active)| {
                let stream_count = active.stream_count();
                let status = if stream_count > 0 {
                    ThreadStatus::Streaming { stream_count }
                } else if active.reservations() > 0 {
                    ThreadStatus::ReservationPending
                } else if active.awaiting_follow_up() {
                    ThreadStatus::AwaitingFollowUp
                } else {
                    ThreadStatus::AttachedIdle
                };
                (
                    thread_id.clone(),
                    ThreadRow {
                        thread_id: thread_id.clone(),
                        account_id: Some(active.account_id.clone()),
                        status,
                        started_at: active.started_at,
                        last_activity_at: Some(active.last_activity_at),
                        request_settings: active.request_settings.clone(),
                    },
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        for (thread_id, attached) in &self.attached_threads {
            if let Entry::Vacant(entry) = rows.entry(thread_id.clone()) {
                entry.insert(ThreadRow {
                    thread_id: thread_id.clone(),
                    account_id: Some(attached.account.clone()),
                    status: ThreadStatus::AttachedIdle,
                    started_at: None,
                    last_activity_at: None,
                    request_settings: ThreadRequestSettings::default(),
                });
            }
        }

        let mut rows = rows.into_values().collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .started_at
                .cmp(&left.started_at)
                .then_with(|| left.thread_id.cmp(&right.thread_id))
        });
        rows
    }
}
