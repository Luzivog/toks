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

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serde_json::json;

    use crate::accounts::{AccountId, ProviderAccount};
    use crate::limits::LimitWindow;

    use super::{account_quota_drain, AccountQuotaDrain};
    use crate::limits::{LimitSnapshot, Provider};
    use crate::rotation::UnixMillis;

    #[test]
    fn account_drain_ignores_scoped_windows_and_uses_the_last_binding_reset() {
        let now = Utc::now();
        let early = now + Duration::hours(2);
        let late = now + Duration::days(4);
        let window = |label: &str, used, reset, scope: Option<&str>| LimitWindow {
            id: label.into(),
            label: label.into(),
            percent_used: used,
            resets_at: Some(reset),
            severity: None,
            scope: scope.map(str::to_owned),
            is_active: false,
            raw: json!({}),
        };
        let snapshot = LimitSnapshot {
            windows: vec![
                window("5-hour", 100.0, early, None),
                window("Weekly", 100.0, late, None),
                window("Weekly - Spark", 100.0, early, Some("Spark")),
            ],
            ..LimitSnapshot::loading_account(
                Provider::Codex,
                ProviderAccount {
                    id: AccountId::new("account"),
                    ..ProviderAccount::unidentified_for(Provider::Codex)
                },
            )
        };

        assert_eq!(
            account_quota_drain(&snapshot, now),
            Some(AccountQuotaDrain {
                account_id: AccountId::new("account"),
                reset_at: Some(UnixMillis::new(late.timestamp_millis())),
            })
        );
    }

    #[test]
    fn scoped_exhaustion_alone_does_not_drain_the_account() {
        let now = Utc::now();
        let snapshot = LimitSnapshot {
            windows: vec![LimitWindow {
                id: "spark".into(),
                label: "Weekly - Spark".into(),
                percent_used: 100.0,
                resets_at: Some(now + Duration::days(1)),
                severity: None,
                scope: Some("Spark".into()),
                is_active: true,
                raw: json!({}),
            }],
            ..LimitSnapshot::loading_account(
                Provider::Codex,
                ProviderAccount {
                    id: AccountId::new("account"),
                    ..ProviderAccount::unidentified_for(Provider::Codex)
                },
            )
        };

        assert_eq!(account_quota_drain(&snapshot, now), None);
    }

    #[test]
    fn account_starts_draining_at_one_percent_remaining() {
        let now = Utc::now();
        let reset = now + Duration::days(1);
        let snapshot = |percent_used| LimitSnapshot {
            windows: vec![LimitWindow {
                id: "weekly".into(),
                label: "Weekly".into(),
                percent_used,
                resets_at: Some(reset),
                severity: None,
                scope: None,
                is_active: true,
                raw: json!({}),
            }],
            ..LimitSnapshot::loading_account(
                Provider::Codex,
                ProviderAccount {
                    id: AccountId::new("account"),
                    ..ProviderAccount::unidentified_for(Provider::Codex)
                },
            )
        };

        assert_eq!(account_quota_drain(&snapshot(98.999), now), None);
        assert_eq!(
            account_quota_drain(&snapshot(99.0), now),
            Some(AccountQuotaDrain {
                account_id: AccountId::new("account"),
                reset_at: Some(UnixMillis::new(reset.timestamp_millis())),
            })
        );
        assert_eq!(
            account_quota_drain(&snapshot(100.0), now),
            Some(AccountQuotaDrain {
                account_id: AccountId::new("account"),
                reset_at: Some(UnixMillis::new(reset.timestamp_millis())),
            })
        );
    }
}
