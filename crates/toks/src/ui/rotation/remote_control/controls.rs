use gpui::{div, prelude::*, SharedString};
use gpui_component::{h_flex, switch::Switch, ActiveTheme, Disableable, Sizable};
use toks_core::remote_control::RemoteConnectionStatus;

use crate::{app::remote_control_operations::RemoteAction, ToksApp};

pub(super) fn controls(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let remote = &app.rotation.remote;
    let status = remote.snapshot.connection.status;
    let busy = remote.busy.is_some();
    let handle = cx.entity().downgrade();
    let toggle = Switch::new(SharedString::from("rotation-remote-toggle"))
        .small()
        .label("Remote control")
        .checked(status != RemoteConnectionStatus::Off)
        .disabled(busy)
        .tooltip("Keep this computer available from your phone")
        .on_click({
            let handle = handle.clone();
            move |checked, _, cx| {
                let action = if *checked {
                    RemoteAction::Enable
                } else {
                    RemoteAction::Disable
                };
                let _ = handle.update(cx, |app, cx| app.run_remote_action(action, cx));
            }
        });
    let mut row = h_flex().items_center().gap_2().child(
        div()
            .debug_selector(|| "rotation-remote-toggle".into())
            .child(toggle),
    );
    if status == RemoteConnectionStatus::Connected {
        row = row.child(
            super::super::super::text_action("rotation-remote-add-device", "Add device", cx)
                .disabled(busy)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.run_remote_action(RemoteAction::Pair, cx);
                })),
        );
    }
    if status == RemoteConnectionStatus::Errored {
        row = row.child(
            super::super::super::text_action("rotation-remote-reconnect", "Reconnect", cx)
                .disabled(busy)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.run_remote_action(RemoteAction::Reconnect, cx);
                })),
        );
    }
    row.when_some(remote.busy.as_ref(), |row, operation| {
        row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(operation_label(operation)),
        )
    })
}

fn operation_label(operation: &super::RemoteOperation) -> &'static str {
    match operation {
        super::RemoteOperation::Enabling => "Turning on",
        super::RemoteOperation::Reconnecting => "Reconnecting",
        super::RemoteOperation::Disabling => "Turning off",
        super::RemoteOperation::Pairing => "Creating code",
        super::RemoteOperation::Revoking(_) => "Removing device",
    }
}
