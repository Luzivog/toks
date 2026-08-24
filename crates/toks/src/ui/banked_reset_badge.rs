use chrono::{DateTime, Utc};
use gpui::{div, prelude::*, SharedString};
use gpui_component::{tooltip::Tooltip, StyledExt};
use toks_core::{LimitSnapshot, Provider};

use super::banked_reset_tooltip::reset_credit_tooltip;

pub(super) fn banked_reset_badge(
    snapshot: &LimitSnapshot,
    now: DateTime<Utc>,
) -> Option<impl IntoElement> {
    let label = banked_reset_label(snapshot.provider, snapshot.banked_resets)?;
    let count = snapshot.banked_resets;
    let credits = snapshot.banked_reset_credits.clone();
    let selector = format!(
        "account-resets-{}-{}",
        snapshot.provider.slug(),
        snapshot.account.id
    );
    let id = selector.clone();
    Some(
        div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .px_1p5()
            .rounded_sm()
            .text_xs()
            .font_medium()
            .bg(gpui::rgb(0x10_a3_7f))
            .text_color(gpui::white())
            .child(label)
            .tooltip(move |window, cx| {
                let credits = credits.clone();
                Tooltip::element(move |_, cx| {
                    reset_credit_tooltip(count, credits.as_deref(), now, cx)
                })
                .p_0()
                .build(window, cx)
            }),
    )
}

pub(super) fn banked_reset_label(provider: Provider, count: u64) -> Option<String> {
    (provider == Provider::Codex && count > 0).then(|| match count {
        1 => "1 reset".to_string(),
        count => format!("{count} resets"),
    })
}
