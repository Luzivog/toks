use gpui::{div, prelude::*, SharedString};
use gpui_component::{h_flex, switch::Switch, Disableable, Sizable};

use crate::{
    app::{RotationServiceAction, SettingsAction},
    ToksApp,
};

pub(super) fn service_controls(
    app: &ToksApp,
    disabled: bool,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let install = &app.rotation.install;
    let fast = app.rotation.settings.fast_when_draining();
    let handle = cx.entity().downgrade();

    let routing = toggle(
        "rotation-routing-toggle",
        "Routing",
        install.configured,
        disabled,
        if install.configured {
            "Turn off to send new Codex sessions directly to Codex"
        } else {
            "Turn on to send new Codex sessions through Toks"
        },
        {
            let handle = handle.clone();
            move |checked, _, cx| {
                let action = if *checked {
                    RotationServiceAction::Enable
                } else {
                    RotationServiceAction::Bypass
                };
                let _ = handle.update(cx, |app, cx| {
                    app.run_rotation_service_action(action, cx);
                });
            }
        },
    );
    let service = toggle(
        "rotation-service-toggle",
        "Router service",
        install.service_active,
        disabled,
        if install.service_active {
            "Turn off to stop and remove the background router service"
        } else if install.service_installed {
            "Turn on to restart the background service and routing"
        } else {
            "Turn on to install the background service and routing"
        },
        {
            let handle = handle.clone();
            move |checked, _, cx| {
                let action = if *checked {
                    RotationServiceAction::Enable
                } else {
                    RotationServiceAction::Disable
                };
                let _ = handle.update(cx, |app, cx| {
                    app.run_rotation_service_action(action, cx);
                });
            }
        },
    );
    let fast_drain = toggle(
        "rotation-fast-drain-toggle",
        "Fast drain",
        fast,
        disabled,
        if fast {
            "Turn off to wait for a reset instead of using faster overage"
        } else {
            "Turn on to finish existing threads faster on a spent account, using more overage"
        },
        move |checked, _, cx| {
            let _ = handle.update(cx, |app, cx| {
                app.change_rotation_settings(SettingsAction::FastWhenDraining(*checked), cx);
            });
        },
    );

    h_flex()
        .gap_4()
        .flex_shrink_0()
        .items_center()
        .child(routing)
        .child(service)
        .child(fast_drain)
}

fn toggle(
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    tooltip: &'static str,
    on_click: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Div {
    div().debug_selector(move || id.to_string()).child(
        Switch::new(SharedString::from(id))
            .small()
            .label(label)
            .checked(checked)
            .disabled(disabled)
            .tooltip(tooltip)
            .on_click(on_click),
    )
}
