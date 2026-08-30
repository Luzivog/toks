use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveThreadRow {
    pub thread_id: ThreadId,
    pub account_id: AccountId,
    pub request_settings: ThreadRequestSettings,
}

impl RotationRuntime {
    fn has_live_thread_presence(
        &self,
        thread_id: &ThreadId,
        active: &super::active_threads::ActiveThread,
    ) -> bool {
        active.stream_count() > 0
            || (active.awaiting_follow_up()
                && self
                    .attached_threads
                    .get(thread_id)
                    .is_some_and(|attached| attached.connections() > 0))
    }

    pub fn live_thread_rows(&self) -> Vec<LiveThreadRow> {
        let mut active_threads = self
            .active_threads
            .iter()
            .filter(|(thread_id, active)| self.has_live_thread_presence(thread_id, active))
            .collect::<Vec<_>>();
        active_threads.sort_by(|(left_id, left), (right_id, right)| {
            right
                .started_at
                .cmp(&left.started_at)
                .then_with(|| left_id.cmp(right_id))
        });
        active_threads
            .into_iter()
            .map(|(thread_id, active)| LiveThreadRow {
                thread_id: thread_id.clone(),
                account_id: active.account_id.clone(),
                request_settings: active.request_settings.clone(),
            })
            .collect()
    }

    pub fn live_thread_count(&self, account: &AccountId) -> u32 {
        self.active_threads
            .iter()
            .filter(|(thread_id, active)| {
                &active.account_id == account && self.has_live_thread_presence(thread_id, active)
            })
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    }

    pub fn thread_request_settings(&self, thread: &ThreadId) -> Option<&ThreadRequestSettings> {
        self.active_threads
            .get(thread)
            .map(|active| &active.request_settings)
    }

    pub(crate) fn retained_thread_ids(&self) -> BTreeSet<ThreadId> {
        self.active_threads
            .keys()
            .chain(self.attached_threads.keys())
            .cloned()
            .collect()
    }
}
