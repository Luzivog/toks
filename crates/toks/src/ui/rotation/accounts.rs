use gpui::{div, prelude::*, px, SharedString};
use gpui_component::{
    h_flex, switch::Switch, v_flex, ActiveTheme, Disableable, Sizable, StyledExt,
};
use toks_core::{rotation::UnixMillis, LimitSnapshot, Provider};

use crate::{app::SettingsAction, ToksApp};

use super::{card, empty_row, format::account_label};

mod banked;
mod controls;
mod state;
use banked::banked_reset_note;
use controls::move_button;
use state::account_state;

pub(super) fn accounts_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let preferred = app.rotation.settings.preferred().cloned();
    let meta = preferred
        .as_ref()
        .map(|id| format!("Override: {}", account_label(app, id)))
        .unwrap_or_else(|| "Highest available first".into());
    let mut panel = card("Account priority", meta, cx);
    if preferred.is_some() {
        panel = panel.child(
            h_flex()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .justify_end()
                .child(
                    super::super::text_action("rotation-clear-preferred", "Clear override", cx)
                        .disabled(app.rotation.busy.is_some())
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.change_rotation_settings(SettingsAction::ClearPreference, cx);
                        })),
                ),
        );
    }
    let snapshots: Vec<_> = app
        .limits
        .iter()
        .filter(|snapshot| snapshot.provider == Provider::Codex)
        .collect();
    if snapshots.is_empty() {
        return panel.child(empty_row(
            "Add a Codex account from Overview before enabling rotation.",
            cx,
        ));
    }
    if app.rotation.settings.priority().is_empty() {
        return panel.child(empty_row("Loading account priority...", cx));
    }
    for (index, account) in app.rotation.settings.priority().iter().enumerate() {
        if let Some(snapshot) = snapshots
            .iter()
            .find(|snapshot| &snapshot.account.id == account)
        {
            panel = panel.child(account_row(app, snapshot, index, snapshots.len(), cx));
        }
    }
    if let Some(note) = banked_reset_note(app, cx) {
        panel = panel.child(note);
    }
    panel
}

fn account_row(
    app: &ToksApp,
    snapshot: &LimitSnapshot,
    index: usize,
    count: usize,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let id = snapshot.account.id.clone();
    let included = !app.rotation.settings.excluded().contains(&id);
    let available = app
        .rotation
        .runtime
        .is_available(&id, UnixMillis::new(app.now.timestamp_millis()));
    let preferred = app.rotation.settings.preferred() == Some(&id);
    let busy = app.rotation.busy.is_some();
    let (state, color) = account_state(app, snapshot, &id, cx);
    let active = app
        .rotation
        .runtime
        .accounts()
        .get(&id)
        .map_or(0, |runtime| runtime.active_streams());
    let switch_id = format!("rotation-included-{id}");
    let switch_account = id.clone();
    let handle = cx.entity().downgrade();

    h_flex()
        .min_h(px(58.))
        .gap_3()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .w(px(22.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{}", index + 1)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .truncate()
                        .child(account_label(app, &id)),
                )
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(div().size(px(6.)).rounded_full().bg(color))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(state),
                        ),
                ),
        )
        .child(
            div()
                .w(px(86.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{active} active")),
        )
        .child(
            h_flex()
                .gap_1()
                .child(move_button(
                    "up",
                    "↑",
                    &id,
                    index.saturating_sub(1),
                    index == 0 || busy,
                    cx,
                ))
                .child(move_button(
                    "down",
                    "↓",
                    &id,
                    index + 1,
                    index + 1 >= count || busy,
                    cx,
                )),
        )
        .child(
            super::super::text_action(
                format!("rotation-use-now-{id}"),
                if preferred { "Using now" } else { "Use now" },
                cx,
            )
            .disabled(!included || !available || preferred || busy)
            .on_click(cx.listener({
                let id = id.clone();
                move |app, _, _, cx| {
                    app.change_rotation_settings(SettingsAction::Prefer(id.clone()), cx);
                }
            })),
        )
        .child(
            Switch::new(SharedString::from(switch_id))
                .small()
                .checked(included)
                .disabled(busy)
                .label("Included")
                .tooltip(if included { "Included" } else { "Excluded" })
                .on_click(move |included, _, cx| {
                    let _ = handle.update(cx, |app, cx| {
                        app.change_rotation_settings(
                            SettingsAction::Include(switch_account.clone(), *included),
                            cx,
                        );
                    });
                }),
        )
}
