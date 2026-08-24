use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, ActiveTheme, StyledExt};
use toks_core::{
    accounts::CredentialProfileKind, remote_control::RemoteConnectionStatus, LimitSnapshot,
    Provider,
};

use crate::ToksApp;

pub(super) fn row(app: &ToksApp, cx: &gpui::App) -> gpui::Div {
    let status = app.rotation.remote.snapshot.connection.status;
    h_flex()
        .debug_selector(|| "rotation-remote-control-row".into())
        .items_center()
        .gap_3()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .items_center()
                .gap_1p5()
                .flex_shrink_0()
                .child(
                    div()
                        .size(px(6.))
                        .rounded_full()
                        .bg(status_color(status, cx)),
                )
                .child(div().text_sm().font_medium().child("Remote control"))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("·"),
                )
                .child(
                    div()
                        .debug_selector(|| "rotation-remote-control-status".into())
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(status_label(status)),
                ),
        )
        .child(
            h_flex()
                .min_w_0()
                .flex_1()
                .justify_end()
                .child(control_identity(app, control_account(app), cx)),
        )
}

pub(super) fn status_label(status: RemoteConnectionStatus) -> &'static str {
    match status {
        RemoteConnectionStatus::Off => "Off",
        RemoteConnectionStatus::Connecting => "Connecting",
        RemoteConnectionStatus::Connected => "On",
        RemoteConnectionStatus::Managed(_) => "On via ChatGPT",
        RemoteConnectionStatus::Errored => "Unavailable",
    }
}

fn status_color(status: RemoteConnectionStatus, cx: &gpui::App) -> gpui::Hsla {
    match status {
        RemoteConnectionStatus::Connected | RemoteConnectionStatus::Managed(_) => {
            gpui::rgb(0x10_a3_7f).into()
        }
        RemoteConnectionStatus::Connecting => cx.theme().warning,
        RemoteConnectionStatus::Errored => cx.theme().danger,
        RemoteConnectionStatus::Off => cx.theme().muted_foreground,
    }
}

fn control_identity(app: &ToksApp, account: Option<&LimitSnapshot>, cx: &gpui::App) -> gpui::Div {
    let Some(account) = account else {
        return div().text_sm().child("Sign in to ChatGPT");
    };
    match account.account.email.as_deref() {
        Some(email) => super::super::account_email::styled_account_email(
            email,
            app.emails_hidden,
            "remote",
            account.account.id.as_str(),
            div().min_w_0().truncate().text_sm(),
            cx,
        ),
        None => div().text_sm().child("Current ChatGPT account"),
    }
}

fn control_account(app: &ToksApp) -> Option<&LimitSnapshot> {
    app.limits.iter().find(|snapshot| {
        snapshot.provider == Provider::Codex
            && snapshot
                .account
                .sources
                .iter()
                .any(|source| source.kind == CredentialProfileKind::Current)
    })
}
