use chrono::{DateTime, Utc};
use gpui::{App, Hsla};
use gpui_component::ActiveTheme;
use toks_core::{
    accounts::AccountId,
    rotation::{account_quota_drain, AccountAvailability, AccountRuntime, UnixMillis},
    LimitSnapshot, LimitWindow,
};

use crate::ToksApp;

pub(super) struct AccountState {
    pub(super) label: String,
    pub(super) color: Hsla,
    pub(super) reset_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DerivedAccountState {
    Available,
    ResetRefreshing,
    Draining {
        until: Option<UnixMillis>,
        reset_known: bool,
    },
    Blocked {
        until: UnixMillis,
        reset_known: bool,
    },
    NeedsSignIn,
}

impl DerivedAccountState {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::ResetRefreshing => "Reset used · Refreshing limits…",
            Self::Draining {
                reset_known: true, ..
            } => "Draining at 1%",
            Self::Draining {
                reset_known: false, ..
            } => "Draining at 1%, rechecking",
            Self::Blocked {
                reset_known: true, ..
            } => "Blocked",
            Self::Blocked {
                reset_known: false, ..
            } => "Blocked, retrying",
            Self::NeedsSignIn => "Needs sign-in",
        }
    }
}

/// The account's general weekly usage window — the one rotation cares about.
/// Model-scoped windows (e.g. `Weekly — GPT-5.3-Codex-Spark`) carry a `scope`
/// and are ignored; the general weekly window is the bare `"Weekly"` label,
/// matching the predicate the `toks-core` limits tests rely on.
pub(super) fn general_weekly_window(snapshot: &LimitSnapshot) -> Option<&LimitWindow> {
    snapshot
        .windows
        .iter()
        .find(|window| window.scope.is_none() && window.label == "Weekly")
}

pub(super) fn account_state(
    app: &ToksApp,
    snapshot: &LimitSnapshot,
    id: &AccountId,
    cx: &App,
) -> AccountState {
    if app.rotation.settings.excluded().contains(id) {
        return state("Excluded", cx.theme().muted_foreground, None, app.now);
    }
    let derived = derive_account_state(
        snapshot,
        app.rotation.runtime.accounts().get(id),
        app.banked_resets.redeemed_at(id),
        app.now,
    );
    match derived {
        DerivedAccountState::Available => state(
            derived.label(),
            gpui::rgb(0x10_a3_7f).into(),
            weekly_reset(app, snapshot),
            app.now,
        ),
        DerivedAccountState::ResetRefreshing => {
            state(derived.label(), cx.theme().muted_foreground, None, app.now)
        }
        DerivedAccountState::NeedsSignIn => {
            state(derived.label(), cx.theme().danger, None, app.now)
        }
        DerivedAccountState::Blocked { until, .. } => state(
            derived.label(),
            cx.theme().danger,
            DateTime::from_timestamp_millis(until.get()),
            app.now,
        ),
        DerivedAccountState::Draining { until, .. } => state(
            derived.label(),
            cx.theme().warning,
            until.and_then(|until| DateTime::from_timestamp_millis(until.get())),
            app.now,
        ),
    }
}

pub(super) fn derive_account_state(
    snapshot: &LimitSnapshot,
    runtime: Option<&AccountRuntime>,
    redeemed_at: Option<UnixMillis>,
    now: DateTime<Utc>,
) -> DerivedAccountState {
    let now_ms = UnixMillis::new(now.timestamp_millis());
    let availability = runtime.map_or(AccountAvailability::Available, |state| {
        state.availability(now_ms)
    });
    if availability == AccountAvailability::NeedsSignIn {
        return DerivedAccountState::NeedsSignIn;
    }
    if let Some(redeemed_at) = redeemed_at {
        let runtime_is_current = runtime.is_some_and(|state| {
            state
                .reset_acknowledged_at()
                .is_some_and(|acknowledged_at| acknowledged_at >= redeemed_at)
        });
        if !runtime_is_current
            && snapshot
                .fetched_at
                .is_some_and(|fetched_at| fetched_at.timestamp_millis() > redeemed_at.get())
        {
            return account_quota_drain(snapshot, now).map_or(
                DerivedAccountState::Available,
                |drain| DerivedAccountState::Draining {
                    until: drain.reset_at,
                    reset_known: drain.reset_at.is_some(),
                },
            );
        }
        if !runtime_is_current
            && matches!(
                availability,
                AccountAvailability::Draining { .. } | AccountAvailability::Blocked { .. }
            )
        {
            return DerivedAccountState::ResetRefreshing;
        }
    }
    match availability {
        AccountAvailability::Available => DerivedAccountState::Available,
        AccountAvailability::NeedsSignIn => DerivedAccountState::NeedsSignIn,
        AccountAvailability::Blocked { until, reset_known } => {
            DerivedAccountState::Blocked { until, reset_known }
        }
        AccountAvailability::Draining { until, reset_known } => DerivedAccountState::Draining {
            until: Some(until),
            reset_known,
        },
    }
}

/// The general weekly window's future reset instant, if any.
fn weekly_reset(app: &ToksApp, snapshot: &LimitSnapshot) -> Option<chrono::DateTime<chrono::Utc>> {
    general_weekly_window(snapshot)
        .and_then(|window| window.resets_at)
        .filter(|reset| *reset > app.now)
}

fn state(
    label: &str,
    color: Hsla,
    reset_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> AccountState {
    let label = reset_at
        .map(|reset| {
            format!(
                "{label} · {}",
                super::super::super::fmt_reset(now, Some(reset))
            )
        })
        .unwrap_or_else(|| label.to_owned());
    AccountState {
        label,
        color,
        reset_at,
    }
}
