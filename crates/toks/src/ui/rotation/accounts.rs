use gpui::prelude::*;
use gpui_component::{h_flex, ActiveTheme, Disableable};
use toks_core::Provider;

use crate::{app::SettingsAction, ToksApp};

use super::{card, empty_row, format::account_label};

mod banked;
mod controls;
mod row;
mod row_status;
mod state;
use banked::banked_reset_note;
use row::account_row;

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
