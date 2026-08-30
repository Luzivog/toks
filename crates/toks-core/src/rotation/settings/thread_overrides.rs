use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{RotationSettings, ThreadId};
use crate::rotation::{RotationEventKind, RotationRuntime, UnixMillis};

const OVERRIDE_RETENTION_MILLIS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThreadOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_tier: Option<String>,
}

impl ThreadOverride {
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    pub fn service_tier(&self) -> Option<&str> {
        self.service_tier.as_deref()
    }

    fn is_empty(&self) -> bool {
        self.model.is_none() && self.reasoning_effort.is_none() && self.service_tier.is_none()
    }

    fn normalize(&mut self) {
        for value in [
            &mut self.model,
            &mut self.reasoning_effort,
            &mut self.service_tier,
        ] {
            if value.as_deref().is_some_and(|value| !valid_value(value)) {
                *value = None;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadOverrideChange {
    Model(Option<String>),
    ReasoningEffort(Option<String>),
    ServiceTier(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidThreadOverrideValue;

impl fmt::Display for InvalidThreadOverrideValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "thread override values must be non-empty, trimmed, and contain no control characters",
        )
    }
}

impl std::error::Error for InvalidThreadOverrideValue {}

impl RotationSettings {
    pub fn thread_overrides(&self) -> &BTreeMap<ThreadId, ThreadOverride> {
        &self.thread_overrides
    }

    pub fn thread_override(&self, thread: &ThreadId) -> Option<&ThreadOverride> {
        self.thread_overrides.get(thread)
    }

    pub fn set_thread_override(
        &mut self,
        thread: &ThreadId,
        change: ThreadOverrideChange,
    ) -> Result<bool, InvalidThreadOverrideValue> {
        let value = match &change {
            ThreadOverrideChange::Model(value)
            | ThreadOverrideChange::ReasoningEffort(value)
            | ThreadOverrideChange::ServiceTier(value) => value.as_deref(),
        };
        if value.is_some_and(|value| !valid_value(value)) {
            return Err(InvalidThreadOverrideValue);
        }

        let mut updated = self
            .thread_overrides
            .get(thread)
            .cloned()
            .unwrap_or_default();
        match change {
            ThreadOverrideChange::Model(value) => updated.model = value,
            ThreadOverrideChange::ReasoningEffort(value) => updated.reasoning_effort = value,
            ThreadOverrideChange::ServiceTier(value) => updated.service_tier = value,
        }
        if updated.is_empty() {
            return Ok(self.thread_overrides.remove(thread).is_some());
        }
        if self.thread_overrides.get(thread) == Some(&updated) {
            return Ok(false);
        }
        self.thread_overrides.insert(thread.clone(), updated);
        Ok(true)
    }

    pub fn reconcile_thread_overrides(
        &mut self,
        runtime: &RotationRuntime,
        now: UnixMillis,
    ) -> bool {
        let mut present = runtime.retained_thread_ids();
        present.extend(runtime.queued_or_resuming_threads());
        let cutoff = now.get().saturating_sub(OVERRIDE_RETENTION_MILLIS);
        let recent = runtime.events().iter().filter_map(|event| {
            (event.at.get() > cutoff)
                .then_some(&event.event)
                .and_then(|event| match event {
                    RotationEventKind::Routed { thread_id, .. } => Some(thread_id),
                    _ => None,
                })
        });
        let recent = recent.cloned().collect::<BTreeSet<_>>();
        let before = self.thread_overrides.len();
        self.thread_overrides
            .retain(|thread, _| present.contains(thread) || recent.contains(thread));
        self.thread_overrides.len() != before
    }

    pub(super) fn normalize_thread_overrides(&mut self) {
        self.thread_overrides
            .values_mut()
            .for_each(ThreadOverride::normalize);
        self.thread_overrides
            .retain(|_, thread_override| !thread_override.is_empty());
    }
}

fn valid_value(value: &str) -> bool {
    !value.is_empty() && value.trim() == value && !value.chars().any(char::is_control)
}
