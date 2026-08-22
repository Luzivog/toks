use chrono::{DateTime, Utc};
use toks_core::{accounts::AccountId, rotation::UnixMillis, Provider};

use crate::ToksApp;

pub(super) fn account_label(app: &ToksApp, account: &AccountId) -> String {
    let codex_accounts: Vec<_> = app
        .limits
        .iter()
        .filter(|snapshot| snapshot.provider == Provider::Codex)
        .collect();
    let Some((index, snapshot)) = codex_accounts
        .iter()
        .enumerate()
        .find(|(_, snapshot)| &snapshot.account.id == account)
    else {
        return "Unknown Codex account".into();
    };
    if app.emails_hidden {
        format!("Codex account {}", index + 1)
    } else {
        snapshot
            .account
            .email
            .clone()
            .unwrap_or_else(|| format!("Codex account {}", index + 1))
    }
}

pub(super) fn exact_time(at: UnixMillis) -> String {
    datetime(at)
        .map(super::super::fmt_exact_local)
        .unwrap_or_else(|| "Unknown time".into())
}

pub(super) fn age(now: DateTime<Utc>, at: UnixMillis) -> String {
    datetime(at)
        .map(|at| super::super::fmt_age(now, at))
        .unwrap_or_else(|| "unknown".into())
}

fn datetime(at: UnixMillis) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(at.get())
}

#[cfg(test)]
mod tests {
    use super::datetime;
    use toks_core::rotation::UnixMillis;

    #[test]
    fn invalid_unix_millis_does_not_invent_a_date() {
        assert!(datetime(UnixMillis::new(i64::MAX)).is_none());
    }
}
