use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::remote_control::{RemoteConnectionStatus, RemoteControlFailureKind};

use crate::ToksApp;

mod account;
mod controls;
mod devices;
mod pairing;

pub(super) use crate::app::remote_control_operations::{RemoteOperation, RemotePanel};

pub(super) fn remote_control_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let remote = &app.rotation.remote;
    let status = remote.snapshot.connection.status;
    let mut card = super::card("Remote control", status_label(status).into(), cx)
        .debug_selector(|| "rotation-remote-control-card".into())
        .child(
            h_flex()
                .debug_selector(|| "rotation-remote-status".into())
                .items_center()
                .gap_3()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(status_color(status, cx)),
                )
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .debug_selector(|| "rotation-remote-server".into())
                                .text_sm()
                                .font_medium()
                                .child(server_label(app)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(description(status)),
                        ),
                )
                .child(controls::controls(app, cx)),
        )
        .child(
            v_flex()
                .px_4()
                .pb_3()
                .child(account::identity_rows(app, cx)),
        );
    if let Some(issue) = &remote.issue {
        card = card.child(
            div()
                .debug_selector(|| "rotation-remote-error".into())
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().danger.opacity(0.5))
                .text_xs()
                .text_color(cx.theme().danger)
                .child(issue_message(issue)),
        );
    }
    if remote.panel == RemotePanel::Pairing {
        if let Some(panel) = pairing::pairing_panel(app, cx) {
            card = card.child(panel);
        }
    } else if remote.panel == RemotePanel::Devices {
        card = card.child(devices::devices_panel(app, cx));
    }
    card
}

fn status_label(status: RemoteConnectionStatus) -> &'static str {
    match status {
        RemoteConnectionStatus::Off => "Off",
        RemoteConnectionStatus::Connecting => "Connecting",
        RemoteConnectionStatus::Connected => "Connected",
        RemoteConnectionStatus::Errored => "Connection error",
    }
}

fn server_label(app: &ToksApp) -> String {
    app.rotation
        .remote
        .snapshot
        .connection
        .server_name
        .clone()
        .unwrap_or_else(|| "This computer".into())
}

fn description(status: RemoteConnectionStatus) -> &'static str {
    match status {
        RemoteConnectionStatus::Off => "Turn on to start and continue Codex tasks from your phone.",
        RemoteConnectionStatus::Connecting => "The secure relay is connecting.",
        RemoteConnectionStatus::Connected => {
            "Phone messages reach this computer; model work follows account priority."
        }
        RemoteConnectionStatus::Errored => {
            "The local host is enabled, but the relay needs attention."
        }
    }
}

fn status_color(status: RemoteConnectionStatus, cx: &gpui::App) -> gpui::Hsla {
    match status {
        RemoteConnectionStatus::Connected => gpui::rgb(0x10_a3_7f).into(),
        RemoteConnectionStatus::Connecting => cx.theme().warning,
        RemoteConnectionStatus::Errored => cx.theme().danger,
        RemoteConnectionStatus::Off => cx.theme().muted_foreground,
    }
}

fn issue_label(kind: RemoteControlFailureKind) -> &'static str {
    match kind {
        RemoteControlFailureKind::SignInRequired => "Sign in to ChatGPT, then try again.",
        RemoteControlFailureKind::VerificationRequired => {
            "Complete account verification in ChatGPT, then try again."
        }
        RemoteControlFailureKind::DisabledByAdministrator => {
            "Remote Control is disabled by your workspace administrator."
        }
        RemoteControlFailureKind::CodexUnavailable => "Install or update Codex, then try again.",
        RemoteControlFailureKind::DaemonUnavailable => "The Codex background host is unavailable.",
        RemoteControlFailureKind::Retryable => "Codex is busy. Try again in a moment.",
        RemoteControlFailureKind::Other => "Remote Control could not complete this action.",
    }
}

fn issue_message(issue: &crate::app::remote_control_operations::RemoteIssue) -> String {
    if issue.kind == RemoteControlFailureKind::Other {
        issue.detail.clone()
    } else {
        issue_label(issue.kind).into()
    }
}
