use gpui::{div, prelude::*};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::{accounts::CredentialProfileKind, LimitSnapshot, Provider};

use crate::ToksApp;

pub(super) fn identity_rows(app: &ToksApp, cx: &gpui::App) -> gpui::Div {
    let account = control_account(app);
    let control_only = account.is_some_and(|snapshot| {
        app.rotation
            .settings
            .excluded()
            .contains(&snapshot.account.id)
    });
    v_flex()
        .gap_1p5()
        .child(identity_row(
            "Connection account",
            control_identity(app, account, cx),
            if control_only {
                "Control only"
            } else {
                "Also used for model work"
            },
            "rotation-remote-control-account",
            cx,
        ))
        .child(identity_row(
            "Model requests",
            div()
                .text_sm()
                .child(super::super::status::selected_account_label(app)),
            "Highest available",
            "rotation-remote-model-account",
            cx,
        ))
}

fn control_identity(app: &ToksApp, account: Option<&LimitSnapshot>, cx: &gpui::App) -> gpui::Div {
    let Some(account) = account else {
        return div().text_sm().child("Sign in to ChatGPT");
    };
    match account.account.email.as_deref() {
        Some(email) => super::super::super::account_email::account_email(
            email,
            app.emails_hidden,
            "remote",
            account.account.id.as_str(),
            cx,
        ),
        None => div().text_sm().child("Current Codex account"),
    }
}

fn identity_row(
    label: &'static str,
    value: gpui::Div,
    meta: &'static str,
    selector: &'static str,
    cx: &gpui::App,
) -> gpui::Div {
    h_flex()
        .debug_selector(move || selector.to_string())
        .min_w_0()
        .gap_3()
        .child(
            div()
                .w_32()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().min_w_0().flex_1().child(value))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(meta),
        )
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
