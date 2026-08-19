use chrono::{DateTime, Utc};
use gpui::{div, prelude::*, px, App, Hsla, SharedString};
use gpui_component::{h_flex, skeleton::Skeleton, tooltip::Tooltip, ActiveTheme, StyledExt};
use tokscope_core::{
    limits::{LimitIssueKind, SnapshotFreshness},
    LimitSnapshot,
};

use super::{fmt_age, fmt_as_of, loading_status};

pub(super) fn limit_header_status(s: &LimitSnapshot, now: DateTime<Utc>, cx: &App) -> gpui::Div {
    if s.status.freshness == SnapshotFreshness::Loading && s.windows.is_empty() {
        return loading_status("Loading usage", cx);
    }

    let age = s.fetched_at.map(|fetched_at| fmt_age(now, fetched_at));
    let exact = s.fetched_at.map(fmt_as_of);
    let saved = !s.windows.is_empty();
    let (label, foreground, background) = if let Some(issue) = &s.status.issue {
        match issue.kind {
            LimitIssueKind::Authentication => (
                "Sign in required".into(),
                cx.theme().danger,
                cx.theme().danger.opacity(0.1),
            ),
            LimitIssueKind::Storage => (
                "Saved · local issue".into(),
                cx.theme().warning,
                cx.theme().warning.opacity(0.1),
            ),
            _ if saved => (
                "Saved · retrying".into(),
                cx.theme().warning,
                cx.theme().warning.opacity(0.1),
            ),
            _ => (
                "Unavailable".into(),
                cx.theme().danger,
                cx.theme().danger.opacity(0.1),
            ),
        }
    } else {
        match s.status.freshness {
            SnapshotFreshness::Live => (
                format!("Updated {}", age.as_deref().unwrap_or("just now")),
                cx.theme().muted_foreground,
                cx.theme().foreground.opacity(0.045),
            ),
            SnapshotFreshness::Cached | SnapshotFreshness::ProviderCache => (
                format!("Saved · {}", age.as_deref().unwrap_or("recently")),
                cx.theme().muted_foreground,
                cx.theme().foreground.opacity(0.045),
            ),
            SnapshotFreshness::Loading => (
                "Refreshing".into(),
                cx.theme().muted_foreground,
                cx.theme().foreground.opacity(0.045),
            ),
            SnapshotFreshness::Unavailable => (
                "Unavailable".into(),
                cx.theme().danger,
                cx.theme().danger.opacity(0.1),
            ),
        }
    };

    let id = format!("account-status-{}-{}", s.provider.slug(), s.account.id);
    status_badge(label, foreground, background, exact, id)
}

fn status_badge(
    label: String,
    foreground: Hsla,
    background: Hsla,
    exact: Option<String>,
    id: String,
) -> gpui::Div {
    let selector = id.clone();
    let badge = div()
        .id(SharedString::from(id))
        .debug_selector(move || selector.clone())
        .px_2()
        .py_1()
        .rounded_md()
        .bg(background)
        .text_xs()
        .font_medium()
        .text_color(foreground)
        .child(label)
        .when_some(exact, |badge, exact| {
            badge.tooltip(move |window, cx| Tooltip::new(exact.clone()).build(window, cx))
        });
    div().flex_shrink_0().child(badge)
}

pub(super) fn pending_limit_row(cx: &App) -> gpui::Div {
    h_flex()
        .h(px(46.))
        .gap_3()
        .px_4()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(Skeleton::new().secondary().w(px(230.)).h_3().rounded_md())
        .child(Skeleton::new().flex_1().h(px(6.)).rounded_full())
        .child(
            h_flex()
                .w(px(230.))
                .flex_shrink_0()
                .justify_end()
                .gap_2()
                .child(Skeleton::new().secondary().w(px(76.)).h_3().rounded_md())
                .child(Skeleton::new().secondary().w(px(132.)).h_3().rounded_md()),
        )
}
