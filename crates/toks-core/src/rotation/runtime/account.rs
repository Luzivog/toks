use crate::rotation::{ThreadId, UnixMillis};
use serde::{Deserialize, Serialize};

use super::{AccountAvailability, AccountRuntime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ThreadUsage {
    StandardOnly { until: UnixMillis },
    Blocked { until: UnixMillis },
}

impl AccountRuntime {
    pub fn blocked_until(&self) -> Option<UnixMillis> {
        self.blocked_until
    }

    pub fn needs_sign_in(&self) -> bool {
        self.needs_sign_in
    }

    pub fn availability(&self, now: UnixMillis) -> AccountAvailability {
        if self.needs_sign_in {
            return AccountAvailability::NeedsSignIn;
        }
        if let Some(until) = self.blocked_until.filter(|until| *until > now) {
            return AccountAvailability::Blocked {
                until,
                reset_known: self.block_reset_known,
            };
        }
        self.quota_drain.filter(|drain| drain.until > now).map_or(
            AccountAvailability::Available,
            |drain| AccountAvailability::Draining {
                until: drain.until,
                reset_known: drain.reset_known,
            },
        )
    }

    pub(super) fn can_drain(&self, thread: &ThreadId, now: UnixMillis) -> bool {
        matches!(
            self.availability(now),
            AccountAvailability::Draining { .. } | AccountAvailability::Blocked { .. }
        ) && self.grandfathered_threads.contains(thread)
            && !matches!(
                self.thread_usage.get(thread),
                Some(ThreadUsage::Blocked { until }) if *until > now
            )
    }

    pub(super) fn requires_standard_tier(&self, thread: &ThreadId, now: UnixMillis) -> bool {
        matches!(
            self.thread_usage.get(thread),
            Some(ThreadUsage::StandardOnly { until }) if *until > now
        )
    }
}
