use toks_core::rotation::{RouterHealth, UnixMillis};

use crate::ToksApp;

use super::super::format::account_label;

pub(super) fn health_label(app: &ToksApp) -> String {
    if app.rotation.install.configured && !app.rotation.install.service_active {
        return "Router service is not running".into();
    }
    if !app.rotation.install.service_active {
        return "Offline".into();
    }
    match app.rotation.runtime.health() {
        RouterHealth::Failed => "Failed, systemd will restart it".into(),
        RouterHealth::Unknown => "Starting".into(),
        RouterHealth::Healthy => app
            .rotation
            .runtime
            .heartbeat_at()
            .map(|at| heartbeat_label(app, at))
            .unwrap_or_else(|| "Starting".into()),
    }
}

fn heartbeat_label(app: &ToksApp, at: UnixMillis) -> String {
    let age = app.now.timestamp_millis().saturating_sub(at.get());
    if age > 15_000 {
        format!("No heartbeat for {}s", age / 1_000)
    } else {
        "Healthy".into()
    }
}

pub(in crate::ui::rotation) fn selected_account_label(app: &ToksApp) -> String {
    if !app.rotation.install.configured || !app.rotation.install.service_active {
        return "Direct Codex connection".into();
    }
    let accounts: Vec<_> = app
        .limits
        .iter()
        .filter(|snapshot| snapshot.provider == toks_core::Provider::Codex)
        .map(|snapshot| snapshot.account.id.clone())
        .collect();
    app.rotation
        .settings
        .select_account(
            &app.rotation.runtime,
            &accounts,
            UnixMillis::new(app.now.timestamp_millis()),
        )
        .map(|id| account_label(app, &id))
        .unwrap_or_else(|| "Waiting for an available account".into())
}
