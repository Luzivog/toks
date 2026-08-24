use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use super::{ThreadId, UnixMillis};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WaitingId(String);

impl WaitingId {
    pub(crate) fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub(crate) fn for_attempt(attempt: &str) -> Self {
        Self::deterministic(b"toks-resume-retry-v1\0", attempt.as_bytes())
    }

    fn legacy(thread: &ThreadId, since: UnixMillis) -> Self {
        let mut identity = thread.as_str().as_bytes().to_vec();
        identity.extend_from_slice(&since.get().to_le_bytes());
        Self::deterministic(b"toks-legacy-waiting-v1\0", &identity)
    }

    fn deterministic(namespace: &[u8], identity: &[u8]) -> Self {
        let mut digest = Sha256::new();
        digest.update(namespace);
        digest.update(identity);
        Self(format!("legacy-{:x}", digest.finalize()))
    }

    pub(crate) fn is_recognized(&self) -> bool {
        is_canonical_uuid(&self.0)
            || self.0.strip_prefix("legacy-").is_some_and(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
    }

    #[cfg(test)]
    pub(crate) fn for_test(value: &str) -> Self {
        Self(value.to_owned())
    }
}

pub(in crate::rotation::runtime) fn is_canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|parsed| parsed.to_string() == value)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingThread {
    pub waiting_id: WaitingId,
    pub thread_id: ThreadId,
    pub since: UnixMillis,
}

impl WaitingThread {
    pub(crate) fn new(thread_id: ThreadId, since: UnixMillis) -> Self {
        Self::with_id(WaitingId::fresh(), thread_id, since)
    }

    pub(crate) fn with_id(waiting_id: WaitingId, thread_id: ThreadId, since: UnixMillis) -> Self {
        Self {
            waiting_id,
            thread_id,
            since,
        }
    }
}

impl super::RotationRuntime {
    /// Attempts currently running in Toks' background resume process. While an
    /// entry is present, that process owns Codex's per-thread writer lock.
    pub fn resuming_threads(&self) -> impl Iterator<Item = &WaitingThread> {
        self.resume_admissions
            .values()
            .filter_map(|admission| admission.active_binding().map(|_| &admission.waiting))
    }

    /// Threads that still belong to the automatic-resume queue, including an
    /// attempt temporarily removed from `waiting_threads` while it runs.
    pub fn queued_or_resuming_threads(&self) -> Vec<ThreadId> {
        let mut pending = self.waiting_threads.clone();
        pending.extend(
            self.resume_admissions
                .values()
                .map(|admission| admission.waiting.clone()),
        );
        pending.sort_by(|left, right| {
            left.since
                .cmp(&right.since)
                .then_with(|| left.waiting_id.cmp(&right.waiting_id))
        });
        let mut seen = BTreeSet::new();
        pending
            .into_iter()
            .filter_map(|waiting| {
                seen.insert(waiting.thread_id.clone())
                    .then_some(waiting.thread_id)
            })
            .collect()
    }
}

impl<'de> Deserialize<'de> for WaitingThread {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Stored {
            #[serde(default)]
            waiting_id: Option<WaitingId>,
            thread_id: ThreadId,
            since: UnixMillis,
        }
        let stored = Stored::deserialize(deserializer)?;
        Ok(Self {
            waiting_id: stored
                .waiting_id
                .unwrap_or_else(|| WaitingId::legacy(&stored.thread_id, stored.since)),
            thread_id: stored.thread_id,
            since: stored.since,
        })
    }
}
