use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

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

impl RotationRuntime {
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
