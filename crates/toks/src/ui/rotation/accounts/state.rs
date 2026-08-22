use gpui::{App, Hsla};
use gpui_component::ActiveTheme;
use toks_core::{accounts::AccountId, LimitSnapshot};

use crate::ToksApp;

pub(super) fn account_state(
    app: &ToksApp,
    snapshot: &LimitSnapshot,
    id: &AccountId,
    cx: &App,
) -> (String, Hsla) {
    if app.rotation.settings.excluded().contains(id) {
        return (
            with_reset("Excluded", app, snapshot),
            cx.theme().muted_foreground,
        );
    }
    if let Some(runtime) = app.rotation.runtime.accounts().get(id) {
        if runtime.needs_sign_in() {
            return (
                with_reset("Needs sign-in", app, snapshot),
                cx.theme().danger,
            );
        }
        if let Some(until) = runtime
            .blocked_until()
            .filter(|until| until.get() > app.now.timestamp_millis())
        {
            return (
                format!("Blocked until {}", super::super::format::exact_time(until)),
                cx.theme().danger,
            );
        }
    }
    let draining = snapshot
        .windows
        .iter()
        .any(|window| !window.reset_elapsed(app.now) && window.percent_remaining() <= f64::EPSILON);
    if draining {
        (
            with_reset("Draining at 0%; active work continues", app, snapshot),
            cx.theme().warning,
        )
    } else {
        (
            with_reset("Available", app, snapshot),
            gpui::rgb(0x10_a3_7f).into(),
        )
    }
}

fn with_reset(label: &str, app: &ToksApp, snapshot: &LimitSnapshot) -> String {
    snapshot
        .windows
        .iter()
        .filter_map(|window| window.resets_at)
        .filter(|reset| *reset > app.now)
        .min()
        .map(|reset| {
            format!(
                "{label}, resets {}",
                super::super::super::fmt_exact_local(reset)
            )
        })
        .unwrap_or_else(|| label.to_owned())
}
