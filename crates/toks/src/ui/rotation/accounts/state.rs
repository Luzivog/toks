use chrono::{DateTime, Utc};
use gpui::{App, Hsla};
use gpui_component::ActiveTheme;
use toks_core::{
    accounts::AccountId,
    rotation::{AccountAvailability, UnixMillis},
    LimitSnapshot, LimitWindow,
};

use crate::ToksApp;

pub(super) struct AccountState {
    pub(super) label: String,
    pub(super) color: Hsla,
    pub(super) reset_at: Option<DateTime<Utc>>,
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
    let now = UnixMillis::new(app.now.timestamp_millis());
    if let Some(runtime) = app.rotation.runtime.accounts().get(id) {
        match runtime.availability(now) {
            AccountAvailability::NeedsSignIn => {
                return state("Needs sign-in", cx.theme().danger, None, app.now);
            }
            AccountAvailability::Blocked { until, reset_known } => {
                let label = if reset_known {
                    "Blocked"
                } else {
                    "Blocked, retrying"
                };
                return state(
                    label,
                    cx.theme().danger,
                    DateTime::from_timestamp_millis(until.get()),
                    app.now,
                );
            }
            AccountAvailability::Draining { until, reset_known } => {
                let label = if reset_known {
                    "Draining at 1%"
                } else {
                    "Draining at 1%, rechecking"
                };
                return state(
                    label,
                    cx.theme().warning,
                    DateTime::from_timestamp_millis(until.get()),
                    app.now,
                );
            }
            AccountAvailability::Available => {}
        }
    }
    state(
        "Available",
        gpui::rgb(0x10_a3_7f).into(),
        weekly_reset(app, snapshot),
        app.now,
    )
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
