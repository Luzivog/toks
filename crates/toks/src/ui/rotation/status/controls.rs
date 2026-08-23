use gpui::{div, prelude::*, SharedString};
use gpui_component::{h_flex, switch::Switch, tooltip::Tooltip, Disableable, Sizable};

use crate::{app::RotationServiceAction, ToksApp};

pub(super) fn routing_controls(
    app: &ToksApp,
    disabled: bool,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let handle = cx.entity().downgrade();

    let routing = toggle(
        "rotation-routing-toggle",
        "Routing",
        app.rotation.settings.enabled(),
        disabled,
        "Route new local Codex work through Toks so it can select an available account",
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
    h_flex()
        .gap_4()
        .flex_shrink_0()
        .items_center()
        .child(routing)
}

fn toggle(
    id: &'static str,
    label: &'static str,
    checked: bool,
    disabled: bool,
    tooltip: &'static str,
    on_click: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let tooltip_selector = format!("{id}-tooltip");
    div()
        .id(SharedString::from(format!("{id}-hover")))
        .debug_selector(move || id.to_string())
        .tooltip(move |window, cx| {
            let selector = tooltip_selector.clone();
            Tooltip::element(move |_, _| {
                let selector = selector.clone();
                div()
                    .debug_selector(move || selector.clone())
                    .child(tooltip)
            })
            .build(window, cx)
        })
        .child(
            Switch::new(SharedString::from(id))
                .small()
                .label(label)
                .checked(checked)
                .disabled(disabled)
                .on_click(on_click),
        )
}
