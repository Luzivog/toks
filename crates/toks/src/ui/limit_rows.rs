use std::rc::Rc;

use chrono::{DateTime, Utc};
use gpui::{div, prelude::*, Context};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::{limits::SnapshotFreshness, LimitSnapshot};

use crate::ToksApp;

mod account_activation;

use super::{
    accent_for_provider,
    account_drag::reorder_handle,
    account_email::account_email,
    account_menu::{
        account_menu, AccountMenuHandlers, AccountRemovalHandler, AccountRemovalState,
        AccountRemovalView,
    },
    banked_reset_badge, limit_header_status, limit_issue_row, pending_limit_row,
    plan_badge::plan_badge_label,
    quota_row,
};

pub(super) struct AccountLimitsGroupContext {
    now: DateTime<Utc>,
    emails_hidden: bool,
    reorder_enabled: bool,
    removal_state: AccountRemovalState,
}

impl AccountLimitsGroupContext {
    pub(super) fn new(
        now: DateTime<Utc>,
        emails_hidden: bool,
        reorder_enabled: bool,
        removal_state: AccountRemovalState,
    ) -> Self {
        Self {
            now,
            emails_hidden,
            reorder_enabled,
            removal_state,
        }
    }
}

pub(super) fn account_limits_group(
    s: &LimitSnapshot,
    group_context: AccountLimitsGroupContext,
    cx: &mut Context<ToksApp>,
) -> gpui::Div {
    let AccountLimitsGroupContext {
        now,
        emails_hidden,
        reorder_enabled,
        removal_state,
    } = group_context;
    let accent = accent_for_provider(s.provider);
    let selector = format!("account-group-{}-{}", s.provider.slug(), s.account.id);
    let header_selector = format!("account-header-{}-{}", s.provider.slug(), s.account.id);
    let app_handle = cx.entity().downgrade();
    let prompt_handle = app_handle.clone();
    let prompt_handler: AccountRemovalHandler = Rc::new(move |key, _, cx| {
        let _ = prompt_handle.update(cx, |app, cx| {
            app.account_removals.confirm(key);
            cx.notify();
        });
    });
    let removal_handler: AccountRemovalHandler = Rc::new(move |key, _, cx| {
        let _ = app_handle.update(cx, |app, cx| {
            crate::app::request_removal(app, key, cx);
        });
    });
    let cancel_handle = cx.entity().downgrade();
    let cancel_handler: AccountRemovalHandler = Rc::new(move |key, _, cx| {
        let _ = cancel_handle.update(cx, |app, cx| {
            app.account_removals.cancel_confirmation(&key);
            cx.notify();
        });
    });
    let activation = account_activation::view(s);
    let (test_handler, automatic_handler) = account_activation::handlers(cx);
    let removal_menu = account_menu(
        AccountRemovalView::new(s.provider, &s.account, removal_state),
        activation,
        AccountMenuHandlers {
            test: test_handler,
            toggle_automatic: automatic_handler,
            prompt_removal: prompt_handler,
            remove: removal_handler,
            cancel_removal: cancel_handler,
        },
        cx,
    );
    let mut group = v_flex()
        .debug_selector(move || selector.clone())
        .w_full()
        .border_t_1()
        .border_color(cx.theme().muted_foreground.opacity(0.22))
        .child(
            h_flex()
                .debug_selector(move || header_selector.clone())
                .bg(accent.opacity(0.04))
                .px_3()
                .py_2p5()
                .justify_between()
                .items_center()
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .min_w_0()
                        .when(reorder_enabled, |row| row.child(reorder_handle(s, cx)))
                        .child(div().size_2().rounded_full().bg(accent).flex_shrink_0())
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap_2()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .whitespace_nowrap()
                                        .child(s.provider.display_name()),
                                )
                                .when_some(s.account.email.as_deref(), |row, email| {
                                    row.child(account_email(
                                        email,
                                        emails_hidden,
                                        s.provider.slug(),
                                        &s.account.id,
                                        cx,
                                    ))
                                })
                                .when_some(s.plan.as_deref(), |row, plan| {
                                    let selector = format!(
                                        "account-plan-{}-{}",
                                        s.provider.slug(),
                                        s.account.id
                                    );
                                    row.child(
                                        div()
                                            .debug_selector(move || selector.clone())
                                            .px_1p5()
                                            .rounded_sm()
                                            .text_xs()
                                            .bg(accent.opacity(0.1))
                                            .text_color(accent.opacity(0.82))
                                            .child(plan_badge_label(plan, s.plan_multiplier)),
                                    )
                                })
                                .when_some(banked_reset_badge(s, now), |row, badge| {
                                    row.child(badge)
                                }),
                        ),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .child(limit_header_status(s, now, cx))
                        .child(removal_menu),
                ),
        );

    if !s.windows.is_empty() {
        for window in &s.windows {
            group = group.child(quota_row(window, now, accent, cx));
        }
    }
    if s.status.freshness == SnapshotFreshness::Loading && s.windows.is_empty() {
        group = group.child(pending_limit_row(cx));
    } else if let Some(issue) = limit_issue_row(s, cx) {
        group = group.child(issue);
    } else if s.windows.is_empty() {
        group = group.child(
            div()
                .px_3()
                .py_2p5()
                .border_t_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("No limit windows reported for this account."),
        );
    }
    group
}
