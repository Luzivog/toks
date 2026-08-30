use gpui::{div, prelude::*, px, SharedString};
use gpui_component::{
    h_flex, switch::Switch, v_flex, ActiveTheme, Disableable, Sizable, StyledExt,
};
use toks_core::LimitSnapshot;

use crate::{app::SettingsAction, ToksApp};

use super::{
    controls::move_button,
    row_status::{account_status, weekly_meter},
    state::account_state,
};
use crate::ui::rotation::format::account_identity;

pub(super) fn account_row(
    app: &ToksApp,
    snapshot: &LimitSnapshot,
    index: usize,
    count: usize,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let id = snapshot.account.id.clone();
    let included = !app.rotation.settings.excluded().contains(&id);
    let busy = app.rotation.busy.is_some();
    let state = account_state(app, snapshot, &id, cx);
    let active = app.rotation.runtime.live_thread_count(&id);
    let switch_account = id.clone();
    let handle = cx.entity().downgrade();
    let meter = weekly_meter(snapshot, cx);

    let identity = h_flex()
        .gap_3()
        .items_center()
        .child(
            div()
                .w(px(20.))
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{}", index + 1)),
        )
        .child(h_flex().flex_1().min_w_0().child(account_identity(
            app,
            &id,
            "rotation-priority",
            div().min_w_0().truncate().text_sm().font_semibold(),
            cx,
        )))
        .when_some(
            super::reset_action::banked_reset_action(app, snapshot, cx),
            |identity, action| identity.child(action),
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
            Switch::new(SharedString::from(format!("rotation-included-{id}")))
                .small()
                .checked(included)
                .disabled(busy)
                .tooltip(if included {
                    "Included in rotation"
                } else {
                    "Excluded from rotation"
                })
                .on_click(move |included, _, cx| {
                    let _ = handle.update(cx, |app, cx| {
                        app.change_rotation_settings(
                            SettingsAction::Include(switch_account.clone(), *included),
                            cx,
                        );
                    });
                }),
        );

    v_flex()
        .gap_2()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(identity)
        .child(account_status(&id, state, meter, active, cx))
}
