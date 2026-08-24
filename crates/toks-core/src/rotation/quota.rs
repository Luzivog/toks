use chrono::{DateTime, Utc};

use crate::accounts::AccountId;
use crate::limits::{LimitSnapshot, Provider};

use super::UnixMillis;

const DRAIN_AT_OR_BELOW_PERCENT_REMAINING: f64 = 1.0;

/// Account-wide drain threshold observed in a Codex limits snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountQuotaDrain {
    pub account_id: AccountId,
    /// `None` means the provider omitted the reset timestamp.
    pub reset_at: Option<UnixMillis>,
}

/// The authority of one account's latest quota refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuotaObservation {
    Draining(Option<UnixMillis>),
    ObservedAvailable,
    Unknown,
}

/// Start draining when an account-wide window has at most 1% remaining.
/// Model-scoped windows do not make the whole subscription unavailable.
pub fn account_quota_drain(
    snapshot: &LimitSnapshot,
    now: DateTime<Utc>,
) -> Option<AccountQuotaDrain> {
    if snapshot.provider != Provider::Codex {
        return None;
    }
    let draining = snapshot.windows.iter().filter(|window| {
        window.scope.is_none()
            && !window.reset_elapsed(now)
            && window.percent_remaining() <= DRAIN_AT_OR_BELOW_PERCENT_REMAINING
    });
    let mut found = false;
    let mut reset_at = None;
    for window in draining {
        found = true;
        let Some(reset) = window.resets_at else {
            return Some(AccountQuotaDrain {
                account_id: snapshot.account.id.clone(),
                reset_at: None,
            });
        };
        let reset = UnixMillis::new(reset.timestamp_millis());
        reset_at = Some(reset_at.map_or(reset, |current: UnixMillis| current.max(reset)));
    }
    found.then(|| AccountQuotaDrain {
        account_id: snapshot.account.id.clone(),
        reset_at,
    })
}
