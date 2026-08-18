use gpui::{div, prelude::*, px, App, Context, Render, SharedString, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use tokscope_core::accounts::{move_account_to, AccountOrderKey};
use tokscope_core::LimitSnapshot;

use crate::TokscopeApp;

#[derive(Clone)]
pub(super) struct AccountDrag {
    key: AccountOrderKey,
    label: String,
}

impl Render for AccountDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary.opacity(0.96))
            .shadow_lg()
            .text_sm()
            .font_semibold()
            .child(self.label.clone())
    }
}

pub(super) fn reorder_handle(snapshot: &LimitSnapshot, cx: &App) -> impl IntoElement {
    let key = AccountOrderKey::from_snapshot(snapshot);
    let selector = format!(
        "account-drag-{}-{}",
        snapshot.provider.slug(),
        snapshot.account.id
    );
    let drag = AccountDrag {
        key,
        label: snapshot.provider.display_name().into(),
    };
    let selector_for_debug = selector.clone();
    div()
        .id(SharedString::from(selector))
        .debug_selector(move || selector_for_debug.clone())
        .w(px(14.))
        .h(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_move()
        .opacity(0.35)
        .hover(|handle| handle.opacity(1.0))
        .child(grip_dots(cx))
        .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
}

fn grip_dots(cx: &App) -> gpui::Div {
    v_flex().gap(px(2.)).children((0..3).map(|_| {
        h_flex()
            .gap(px(2.))
            .children((0..2).map(|_| div().size(px(2.)).rounded_full().bg(cx.theme().foreground)))
    }))
}

pub(super) fn account_drop_target(
    snapshot: &LimitSnapshot,
    child: gpui::Div,
    cx: &mut Context<TokscopeApp>,
) -> gpui::Div {
    let key = AccountOrderKey::from_snapshot(snapshot);
    let drop_key = key.clone();
    let selector = format!(
        "account-drop-{}-{}",
        snapshot.provider.slug(),
        snapshot.account.id
    );
    div()
        .debug_selector(move || selector.clone())
        .w_full()
        .drag_over::<AccountDrag>(move |style, drag, _, cx| {
            if drag.key == key {
                style
            } else {
                style.bg(cx.theme().sidebar_accent)
            }
        })
        .on_drop(cx.listener(move |app, drag: &AccountDrag, _, cx| {
            if let Err(error) = move_account_to(&mut app.limits, &drag.key, &drop_key) {
                app.account_notice = Some(format!("Couldn't save account order: {error}"));
            }
            cx.notify();
        }))
        .child(child)
}
