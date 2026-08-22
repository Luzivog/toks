use crate::rotation::{ThreadId, UnixMillis};

use super::{AccountAvailability, AccountRuntime};

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
        self.quota_exhaustion
            .filter(|exhaustion| exhaustion.until > now)
            .map_or(AccountAvailability::Available, |exhaustion| {
                AccountAvailability::Draining {
                    until: exhaustion.until,
                    reset_known: exhaustion.reset_known,
                }
            })
    }

    pub(super) fn can_drain(&self, thread: &ThreadId, now: UnixMillis) -> bool {
        matches!(self.availability(now), AccountAvailability::Draining { .. })
            && self.grandfathered_threads.contains(thread)
    }
}
