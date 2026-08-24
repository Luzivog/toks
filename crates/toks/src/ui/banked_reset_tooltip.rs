use chrono::{DateTime, Utc};
use gpui::{div, prelude::*, px, App};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::limits::{BankedResetCredit, BankedResetCreditStatus};

use super::fmt_exact_local;

const REDEMPTION_NOTE: &str =
    "Redeeming one reset resets both the Codex 5-hour and weekly usage windows.";

pub(super) fn reset_credit_tooltip(
    count: u64,
    credits: Option<&[BankedResetCredit]>,
    now: DateTime<Utc>,
    cx: &App,
) -> gpui::Div {
    let mut tooltip = v_flex()
        .debug_selector(|| "banked-reset-tooltip".to_string())
        .w(px(340.))
        .gap_2()
        .p_3()
        .child(div().text_sm().font_semibold().child("Banked resets"))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(REDEMPTION_NOTE),
        );
    let Some(credits) = credits else {
        return tooltip.child(detail_notice("Expiry details unavailable", cx));
    };
    let credits = visible_credits(credits, count);
    for (index, credit) in credits.iter().enumerate() {
        tooltip = tooltip.child(credit_row(index, credit, now, cx));
    }
    let missing = count.saturating_sub(credits.len() as u64);
    if missing > 0 {
        let noun = if missing == 1 { "date" } else { "dates" };
        tooltip = tooltip.child(detail_notice(
            &format!("{missing} expiry {noun} unavailable"),
            cx,
        ));
    }
    tooltip
}

fn credit_row(index: usize, credit: &BankedResetCredit, now: DateTime<Utc>, cx: &App) -> gpui::Div {
    let title = credit.title.as_deref().unwrap_or("Codex reset");
    let title = match credit.status {
        Some(status) => format!("{title} · {}", status.display_name()),
        None => title.to_string(),
    };
    let expiry = expiry_label(credit, now);
    v_flex()
        .debug_selector(move || format!("banked-reset-credit-{index}"))
        .gap_0p5()
        .pt_1()
        .child(div().text_sm().font_medium().child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(expiry),
        )
}

pub(super) fn expiry_label(credit: &BankedResetCredit, now: DateTime<Utc>) -> String {
    credit.expires_at.map_or_else(
        || "Does not expire".into(),
        |expires_at| {
            format!(
                "{} · {}",
                fmt_exact_local(expires_at),
                relative_expiry(now, expires_at)
            )
        },
    )
}

fn detail_notice(text: &str, cx: &App) -> gpui::Div {
    h_flex()
        .debug_selector(|| "banked-reset-details-unavailable".to_string())
        .pt_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}

pub(super) fn visible_credits(
    credits: &[BankedResetCredit],
    count: u64,
) -> Vec<&BankedResetCredit> {
    let mut credits: Vec<_> = credits
        .iter()
        .filter(|credit| credit.status == Some(BankedResetCreditStatus::Available))
        .collect();
    credits.sort_by(|left, right| match (left.expires_at, right.expires_at) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    credits.truncate(usize::try_from(count).unwrap_or(usize::MAX));
    credits
}

pub(super) fn relative_expiry(now: DateTime<Utc>, at: DateTime<Utc>) -> String {
    let seconds = (at - now).num_seconds();
    let prefix = if seconds >= 0 {
        "expires in"
    } else {
        "expired"
    };
    let suffix = if seconds >= 0 { "" } else { " ago" };
    let minutes = seconds.unsigned_abs().div_ceil(60);
    let duration = if minutes >= 24 * 60 {
        format!("{}d {}h", minutes / (24 * 60), (minutes % (24 * 60)) / 60)
    } else if minutes >= 60 {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    } else {
        format!("{}m", minutes.max(1))
    };
    format!("{prefix} {duration}{suffix}")
}
