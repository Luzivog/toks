use gpui::prelude::*;
use toks_core::Provider;

use crate::ToksApp;

use super::{card, empty_row};

mod banked;
mod controls;
mod reset_action;
mod row;
mod row_status;
mod state;
use banked::{banked_reset_note, banked_reset_result};
use row::account_row;

pub(super) fn accounts_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let mut panel = card("Account priority", "Highest available first".into(), cx);
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
    if let Some(result) = banked_reset_result(app, cx) {
        panel = panel.child(result);
    }
    if let Some(note) = banked_reset_note(app, cx) {
        panel = panel.child(note);
    }
    panel
}
