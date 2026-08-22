use chrono::{DateTime, Utc};
use gpui::{div, prelude::*};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::remote_control::{RemoteDevice, RemoteDevices};

use crate::{app::remote_control_operations::RemoteAction, ToksApp};

pub(super) fn devices_panel(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let mut panel = v_flex()
        .debug_selector(|| "rotation-remote-devices-panel".into())
        .gap_2()
        .px_4()
        .py_3()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .justify_between()
                .child(div().text_sm().font_medium().child("Paired devices"))
                .child(
                    super::super::super::text_action("rotation-remote-close-devices", "Done", cx)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.rotation.remote.panel = super::RemotePanel::Summary;
                            app.rotation.remote.pending_revoke = None;
                            cx.notify();
                        })),
                ),
        );
    panel = match &app.rotation.remote.snapshot.devices {
        RemoteDevices::NotLoaded => panel.child(notice("Loading devices…", cx)),
        RemoteDevices::Failed(error) => panel.child(notice(error, cx)),
        RemoteDevices::Loaded(devices) if devices.is_empty() => {
            panel.child(notice("No paired devices.", cx))
        }
        RemoteDevices::Loaded(devices) => devices.iter().fold(panel, |panel, device| {
            panel.child(device_row(app, device, cx))
        }),
    };
    panel
}

fn device_row(app: &ToksApp, device: &RemoteDevice, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let id = device.client_id.clone();
    let confirming = app.rotation.remote.pending_revoke.as_deref() == Some(id.as_str());
    let title = device.display_name.as_deref().unwrap_or("Remote device");
    let detail = device_detail(device, app.now);
    let actions = if confirming {
        let cancel_id = id.clone();
        let revoke_id = id.clone();
        h_flex()
            .gap_1()
            .child(
                super::super::super::text_action(
                    format!("rotation-remote-cancel-remove-{id}"),
                    "Cancel",
                    cx,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    if app.rotation.remote.pending_revoke.as_deref() == Some(cancel_id.as_str()) {
                        app.rotation.remote.cancel_revoke();
                        cx.notify();
                    }
                })),
            )
            .child(
                super::super::super::text_action(
                    format!("rotation-remote-confirm-remove-{id}"),
                    "Remove device",
                    cx,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.run_remote_action(RemoteAction::Revoke(revoke_id.clone()), cx);
                })),
            )
    } else {
        let confirm_id = id.clone();
        h_flex().child(
            super::super::super::text_action(format!("rotation-remote-remove-{id}"), "Remove", cx)
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.rotation.remote.confirm_revoke(confirm_id.clone());
                    cx.notify();
                })),
        )
    };
    h_flex()
        .debug_selector(move || format!("rotation-remote-device-{id}"))
        .min_w_0()
        .justify_between()
        .gap_3()
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().truncate().child(title.to_string()))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(detail),
                ),
        )
        .child(actions)
}

fn device_detail(device: &RemoteDevice, now: DateTime<Utc>) -> String {
    let platform = device.platform.as_deref().or(device.device_type.as_deref());
    let seen = device
        .last_seen_at
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        .map(|at| super::super::super::fmt_age(now, at));
    [platform.map(str::to_string), seen]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

fn notice(message: impl Into<String>, cx: &gpui::App) -> gpui::Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(message.into())
}
