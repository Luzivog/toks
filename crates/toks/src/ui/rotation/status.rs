use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, StyledExt};
use toks_core::rotation::{RouterHealth, UnixMillis};

use crate::{app::RotationServiceAction, ToksApp};

use super::{card, format::account_label};

pub(super) fn service_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let install = &app.rotation.install;
    let service = if install.configured && install.service_active {
        "Routing active"
    } else if install.configured {
        "Service stopped"
    } else if install.service_installed {
        "Bypassed"
    } else {
        "Not enabled"
    };
    let health = health_label(app);
    let selected = selected_account_label(app);
    let busy = app.rotation.busy.is_some();

    card("Router", service.into(), cx)
        .child(
            v_flex()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(detail_row("Service", service, cx))
                .child(detail_row("Health", &health, cx))
                .child(detail_row("New work", &selected, cx)),
        )
        .child(
            h_flex()
                .gap_2()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .when(!install.configured, |actions| {
                    actions.child(service_action(
                        "rotation-enable",
                        "Enable routing",
                        RotationServiceAction::Enable,
                        busy,
                        cx,
                    ))
                })
                .when(install.configured && !install.service_active, |actions| {
                    actions.child(service_action(
                        "rotation-restart",
                        "Restart router",
                        RotationServiceAction::Enable,
                        busy,
                        cx,
                    ))
                })
                .when(install.configured, |actions| {
                    actions.child(service_action(
                        "rotation-bypass",
                        "Bypass routing",
                        RotationServiceAction::Bypass,
                        busy,
                        cx,
                    ))
                })
                .when(install.service_installed, |actions| {
                    actions.child(service_action(
                        "rotation-disable",
                        "Disable service",
                        RotationServiceAction::Disable,
                        busy,
                        cx,
                    ))
                })
                .when_some(app.rotation.busy, |actions, label| {
                    actions.child(
                        div()
                            .ml_auto()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    )
                }),
        )
        .when(!install.configured, |panel| {
            panel.child(
                div()
                    .px_4()
                    .pb_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Enabling changes Codex configuration for future local processes. Restart Codex Desktop and existing CLI sessions after enabling.",
                    ),
            )
        })
}

fn detail_row(label: &'static str, value: &str, cx: &gpui::App) -> gpui::Div {
    h_flex()
        .min_h(px(34.))
        .px_4()
        .justify_between()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().text_sm().font_medium().child(value.to_owned()))
}

fn service_action(
    id: &'static str,
    label: &'static str,
    action: RotationServiceAction,
    disabled: bool,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui_component::button::Button {
    super::super::text_action(id, label, cx)
        .disabled(disabled)
        .on_click(cx.listener(move |app, _, _, cx| {
            app.run_rotation_service_action(action, cx);
        }))
}

fn health_label(app: &ToksApp) -> String {
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

fn selected_account_label(app: &ToksApp) -> String {
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
