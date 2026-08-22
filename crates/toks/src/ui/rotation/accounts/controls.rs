use gpui_component::Disableable;
use toks_core::accounts::AccountId;

use crate::{app::SettingsAction, ToksApp};

pub(super) fn move_button(
    direction: &'static str,
    label: &'static str,
    account: &AccountId,
    index: usize,
    disabled: bool,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui_component::button::Button {
    let id = account.clone();
    super::super::super::text_action(format!("rotation-{direction}-{account}"), label, cx)
        .compact()
        .disabled(disabled)
        .tooltip(format!("Move {direction}"))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.change_rotation_settings(SettingsAction::MoveAccount(id.clone(), index), cx);
        }))
}
