use gpui::{div, prelude::*};
use gpui_component::{h_flex, Disableable};
use toks_core::LimitSnapshot;

use crate::{
    app::banked_reset_operations::{request_banked_reset, BankedResetStatus},
    ToksApp,
};

pub(super) fn banked_reset_action(
    app: &ToksApp,
    snapshot: &LimitSnapshot,
    cx: &mut gpui::Context<ToksApp>,
) -> Option<gpui::Div> {
    let id = snapshot.account.id.clone();
    let status = app.banked_resets.status(&id);
    let derived = super::state::derive_account_state(
        snapshot,
        app.rotation.runtime.accounts().get(&id),
        app.banked_resets.redeemed_at(&id),
        app.now,
    );
    let resettable = matches!(
        derived,
        super::state::DerivedAccountState::Draining { .. }
            | super::state::DerivedAccountState::Blocked { .. }
    );
    if matches!(status, BankedResetStatus::Ready | BankedResetStatus::Busy)
        && (snapshot.banked_resets == 0 || !resettable)
    {
        return None;
    }
    let action = match status {
        BankedResetStatus::Ready => {
            super::super::super::text_action(format!("rotation-use-reset-{id}"), "Use reset", cx)
                .on_click(cx.listener({
                    let id = id.clone();
                    move |app, _, _, cx| {
                        app.banked_resets.confirm(id.clone());
                        cx.notify();
                    }
                }))
                .into_any_element()
        }
        BankedResetStatus::Busy => {
            super::super::super::text_action(format!("rotation-use-reset-{id}"), "Use reset", cx)
                .disabled(true)
                .into_any_element()
        }
        BankedResetStatus::Confirming => {
            let cancel_id = id.clone();
            let account = snapshot.account.clone();
            let count = snapshot.banked_resets;
            h_flex()
                .gap_1()
                .items_center()
                .child(div().text_xs().child("Reset both limits?"))
                .child(
                    super::super::super::text_action(
                        format!("rotation-cancel-reset-{id}"),
                        "Cancel",
                        cx,
                    )
                    .compact()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.banked_resets.cancel(&cancel_id);
                        cx.notify();
                    })),
                )
                .child(
                    super::super::super::text_action(
                        format!("rotation-confirm-reset-{id}"),
                        "Use 1 reset",
                        cx,
                    )
                    .compact()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        request_banked_reset(app, account.clone(), count, cx);
                    })),
                )
                .into_any_element()
        }
        BankedResetStatus::Pending => super::super::super::text_action(
            format!("rotation-pending-reset-{id}"),
            "Using reset",
            cx,
        )
        .loading(true)
        .disabled(true)
        .into_any_element(),
        BankedResetStatus::Retry(message) => {
            let account = snapshot.account.clone();
            let count = snapshot.banked_resets;
            let cancel_id = id.clone();
            h_flex()
                .gap_1()
                .items_center()
                .child(div().text_xs().child("Couldn't confirm"))
                .child(
                    super::super::super::text_action(
                        format!("rotation-retry-reset-{id}"),
                        "Retry safely",
                        cx,
                    )
                    .compact()
                    .tooltip(message)
                    .on_click(cx.listener(move |app, _, _, cx| {
                        request_banked_reset(app, account.clone(), count, cx);
                    })),
                )
                .child(
                    super::super::super::text_action(
                        format!("rotation-cancel-reset-{id}"),
                        "Cancel",
                        cx,
                    )
                    .compact()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.banked_resets.cancel(&cancel_id);
                        cx.notify();
                    })),
                )
                .into_any_element()
        }
    };
    Some(div().flex_shrink_0().child(action))
}
