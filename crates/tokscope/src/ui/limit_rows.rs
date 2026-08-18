use chrono::{DateTime, Utc};
use gpui::{div, prelude::*, px, relative, App, Context};
use gpui_component::{box_shadow, h_flex, v_flex, ActiveTheme, StyledExt};
use tokscope_core::{limits::SnapshotFreshness, LimitSnapshot};

use crate::TokscopeApp;

use super::{
    accent_for_provider, account_drag::reorder_handle, limit_header_status, limit_issue_row,
    pending_limit_row, plan_badge::plan_badge_label, quota_row,
};

pub(super) fn account_limits_group(
    s: &LimitSnapshot,
    now: DateTime<Utc>,
    separated: bool,
    emails_hidden: bool,
    reorder_enabled: bool,
    cx: &mut Context<TokscopeApp>,
) -> gpui::Div {
    let accent = accent_for_provider(s.provider);
    let selector = format!("account-group-{}-{}", s.provider.slug(), s.account.id);
    let mut group = v_flex()
        .debug_selector(move || selector.clone())
        .w_full()
        .when(separated, |group| {
            group
                .border_t_1()
                .border_color(cx.theme().muted_foreground.opacity(0.22))
        })
        .child(
            h_flex()
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
                                }),
                        ),
                )
                .child(limit_header_status(s, now, cx)),
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

fn account_email(
    email: &str,
    hidden: bool,
    provider: &str,
    account_id: &str,
    cx: &App,
) -> gpui::Div {
    let selector = format!("account-email-{provider}-{account_id}");
    let blur_selector = format!("account-email-blur-{provider}-{account_id}");
    div()
        .debug_selector(move || selector.clone())
        .relative()
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(email.to_string()),
        )
        .when(hidden, |email| email.child(privacy_blur(blur_selector, cx)))
}

fn privacy_blur(selector: String, cx: &App) -> gpui::Div {
    let haze = cx.theme().muted_foreground.opacity(0.2);
    let soft_shadow = box_shadow(px(0.), px(0.), px(6.), px(1.), haze);
    div()
        .debug_selector(move || selector.clone())
        .absolute()
        .top(px(-1.))
        .bottom(px(-1.))
        .left(px(-2.))
        .right(px(-2.))
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().foreground.opacity(0.045))
        .bg(cx.theme().secondary.opacity(0.9))
        .children(
            [
                (0.05_f32, 0.34_f32, 0.24_f32),
                (0.35, 0.28, 0.16),
                (0.64, 0.31, 0.2),
            ]
            .map(|(left, width, opacity)| {
                div()
                    .absolute()
                    .left(relative(left))
                    .top(px(6.))
                    .w(relative(width))
                    .h(px(5.))
                    .rounded_full()
                    .bg(cx.theme().muted_foreground.opacity(opacity))
                    .shadow(vec![soft_shadow.clone()])
            }),
        )
}
