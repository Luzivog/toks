use gpui::{div, prelude::*, px, Corner};
use gpui_component::{
    h_flex,
    menu::{DropdownMenu, PopupMenuItem},
    v_flex, ActiveTheme, StyledExt,
};
use toks_core::{LimitSnapshot, Provider};

use crate::app::RemovalStatus;
use crate::window::{icon_element, ToksIcon};
use crate::ToksApp;

use super::{
    account_drop_target, account_email::email_visibility_button, account_error_rows,
    account_limits_group, account_limits_loading_content, action_button, section_title,
};

pub(super) fn account_limits_section(
    app: &ToksApp,
    title: &'static str,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let app_handle = cx.entity().downgrade();
    let has_emails = app
        .limits
        .iter()
        .any(|snapshot| snapshot.account.email.is_some());
    let toggle_emails = email_visibility_button("toggle-account-emails", app.emails_hidden, cx);
    let add_account = action_button("add-account", cx)
        .compact()
        .child(icon_element(ToksIcon::Plus).size(px(13.)))
        .child(div().text_sm().child("Add account"))
        .dropdown_menu_with_anchor(Corner::TopRight, move |mut menu, _, _| {
            menu = menu.label("Choose a provider").min_w(px(180.));
            for choice in Provider::ALL {
                let app_handle = app_handle.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .debug_selector(move || {
                                format!("add-account-provider-{}", choice.slug())
                            })
                            .size_full()
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child(choice.display_name())
                    })
                    .on_click(move |_, _, cx| {
                        let result = toks_core::accounts::begin_add_account(choice);
                        let _ = app_handle.update(cx, |app, cx| {
                            match result {
                                Ok(started) => {
                                    let started_at = chrono::Utc::now();
                                    app.account_operations.start_add(
                                        started.provider,
                                        started.account_id,
                                        started_at,
                                        &mut app.limits,
                                    );
                                    app.account_operations
                                        .reconcile(&mut app.limits, started_at);
                                }
                                Err(error) => app.account_operations.report_error(format!(
                                    "Couldn't add {}: {error}",
                                    choice.display_name()
                                )),
                            }
                            cx.notify();
                        });
                    }),
                );
            }
            menu
        });

    let mut panel = v_flex()
        .w_full()
        .overflow_hidden()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .px_3()
                .py_2()
                .justify_between()
                .items_center()
                .child(section_title(title))
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .when(has_emails, |actions| actions.child(toggle_emails))
                        .child(add_account),
                ),
        );

    if let Some(notice) = &app.account_notice {
        panel = panel.child(
            div()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(notice.clone()),
        );
    }
    panel = panel.children(account_error_rows(app, cx));
    let snapshots: Vec<&LimitSnapshot> = app.limits.iter().collect();
    if snapshots.is_empty() {
        if app.limits_loaded {
            return panel.child(
                v_flex()
                    .gap_1()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().font_semibold().child("No account data"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Add an account to see its plan limits here."),
                    ),
            );
        }

        return panel.child(account_limits_loading_content(cx));
    }

    let reorder_enabled = snapshots.len() > 1;
    for snapshot in snapshots {
        let removal_key = toks_core::accounts::AccountOrderKey::from_snapshot(snapshot);
        let removal_state = match app.account_removals.status(&removal_key) {
            RemovalStatus::Ready => super::account_menu::AccountRemovalState::Ready,
            RemovalStatus::Confirming => super::account_menu::AccountRemovalState::Confirming,
            RemovalStatus::Pending => super::account_menu::AccountRemovalState::Pending,
            RemovalStatus::Failed(message) => {
                super::account_menu::AccountRemovalState::Failed(message.into())
            }
        };
        let group = account_limits_group(
            snapshot,
            app.now,
            true,
            app.emails_hidden,
            reorder_enabled,
            removal_state,
            cx,
        );
        panel = if reorder_enabled {
            panel.child(account_drop_target(snapshot, group, cx))
        } else {
            panel.child(group)
        };
    }
    panel
}
