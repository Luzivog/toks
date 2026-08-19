use gpui::{div, prelude::*, Context};
use gpui_component::{h_flex, ActiveTheme};
use toks_core::{limits::LimitIssueKind, LimitSnapshot};

use crate::ToksApp;

use super::text_action;

pub(super) fn limit_issue_row(
    snapshot: &LimitSnapshot,
    cx: &mut Context<ToksApp>,
) -> Option<gpui::Div> {
    let issue = snapshot.status.issue.as_ref()?;
    let saved = !snapshot.windows.is_empty();
    if saved
        && matches!(
            issue.kind,
            LimitIssueKind::RateLimited
                | LimitIssueKind::Network
                | LimitIssueKind::InvalidResponse
                | LimitIssueKind::Unavailable
        )
    {
        return None;
    }
    let message = issue_message(issue.kind, saved);
    let reauthenticate = (issue.kind == LimitIssueKind::Authentication)
        .then(|| snapshot.account.primary_source())
        .flatten()
        .map(|source| reauthentication_action(snapshot, source.profile_id.clone(), cx));

    Some(
        h_flex()
            .px_3()
            .py_1p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .justify_between()
            .items_center()
            .gap_3()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(div().min_w_0().child(message))
            .when_some(reauthenticate, |row, action| row.child(action)),
    )
}

fn issue_message(kind: LimitIssueKind, saved: bool) -> &'static str {
    match kind {
        LimitIssueKind::Authentication => "Sign in again to refresh this account.",
        LimitIssueKind::RateLimited if saved => {
            "Provider rate limit reached. Showing saved usage while Toks retries."
        }
        LimitIssueKind::RateLimited => "Provider rate limit reached. Toks will retry.",
        LimitIssueKind::Network if saved => {
            "Couldn't refresh right now. Showing the last saved usage."
        }
        LimitIssueKind::Network => "Couldn't load usage right now. Toks will retry.",
        LimitIssueKind::InvalidResponse if saved => {
            "The latest response was unreadable. Showing the last saved usage."
        }
        LimitIssueKind::InvalidResponse => "The provider returned an unreadable response.",
        LimitIssueKind::Storage => "Couldn't save the latest usage locally.",
        LimitIssueKind::Unavailable if saved => "Showing the last saved usage.",
        LimitIssueKind::Unavailable => "Usage is unavailable for this account.",
    }
}

fn reauthentication_action(
    snapshot: &LimitSnapshot,
    profile_id: toks_core::accounts::CredentialProfileId,
    cx: &mut Context<ToksApp>,
) -> impl gpui::IntoElement {
    let provider = snapshot.provider;
    let selector = format!("reauthenticate-{}-{}", provider.slug(), snapshot.account.id);
    text_action(selector, "Sign in again", cx)
        .compact()
        .flex_shrink_0()
        .on_click(cx.listener(move |app, _, _, cx| {
            match toks_core::accounts::begin_reauthentication(provider, &profile_id) {
                Ok(()) => {
                    let started_at = chrono::Utc::now();
                    app.account_operations.start_reauthentication(
                        provider,
                        profile_id.clone(),
                        started_at,
                    );
                    app.account_operations
                        .reconcile(&mut app.limits, started_at);
                }
                Err(error) => app.account_operations.report_error(format!(
                    "Couldn't sign in to {}: {error}",
                    provider.display_name()
                )),
            }
            cx.notify();
        }))
}
