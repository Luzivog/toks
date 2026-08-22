use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};

use crate::ToksApp;

mod controls;
mod state;
use controls::service_controls;
use state::{health_label, selected_account, SelectedAccount};

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
    let selected = selected_account_identity(app, "rotation-selected", cx);
    let busy = app.rotation.busy.is_some();
    let status_color = if install.configured && install.service_active {
        gpui::rgb(0x10_a3_7f).into()
    } else if install.configured {
        cx.theme().danger
    } else if install.service_installed {
        cx.theme().warning
    } else {
        cx.theme().muted_foreground
    };

    v_flex()
        .w_full()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .child(
            h_flex()
                .debug_selector(|| "rotation-router-controls".into())
                .items_center()
                .gap_3()
                .px_4()
                .py_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .flex_shrink_0()
                        .child(div().size(px(6.)).rounded_full().bg(status_color))
                        .child(div().text_sm().font_medium().child(service))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("·"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(health),
                        ),
                )
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("New work"),
                        )
                        .child(h_flex().min_w_0().flex_1().child(selected)),
                )
                .child(service_controls(app, busy, cx))
                .when_some(app.rotation.busy, |row, label| {
                    row.child(
                        div()
                            .flex_shrink_0()
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
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Enabling changes Codex configuration for future local processes. Restart Codex Desktop and existing CLI sessions after enabling.",
                    ),
            )
        })
}

pub(in crate::ui::rotation) fn selected_account_identity(
    app: &ToksApp,
    surface: &str,
    cx: &gpui::App,
) -> gpui::Div {
    match selected_account(app) {
        SelectedAccount::Direct => div().text_sm().child("Direct Codex connection"),
        SelectedAccount::Account(account) => super::format::account_identity(
            app,
            &account,
            surface,
            div().min_w_0().truncate().text_sm(),
            cx,
        ),
        SelectedAccount::Waiting => div().text_sm().child("Waiting for an available account"),
    }
}
