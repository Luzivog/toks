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

fn expiry_label(credit: &BankedResetCredit, now: DateTime<Utc>) -> String {
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

fn visible_credits(credits: &[BankedResetCredit], count: u64) -> Vec<&BankedResetCredit> {
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

fn relative_expiry(now: DateTime<Utc>, at: DateTime<Utc>) -> String {
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

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};
    use toks_core::limits::{BankedResetCredit, BankedResetCreditStatus};

    use super::{expiry_label, relative_expiry, visible_credits};

    #[test]
    fn only_available_credits_render_in_expiry_order_up_to_the_usage_count() {
        let at = |hour| Utc.with_ymd_and_hms(2026, 8, 22, hour, 0, 0).single();
        let credits = [
            credit(at(9), Some(BankedResetCreditStatus::Redeemed)),
            credit(at(10), None),
            credit(None, Some(BankedResetCreditStatus::Available)),
            credit(at(15), Some(BankedResetCreditStatus::Available)),
            credit(at(13), Some(BankedResetCreditStatus::Available)),
        ];
        let visible = visible_credits(&credits, 2);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].expires_at, at(13));
        assert_eq!(visible[1].expires_at, at(15));
    }

    #[test]
    fn relative_expiry_handles_future_and_elapsed_credits() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            relative_expiry(now, now + chrono::Duration::minutes(90)),
            "expires in 1h 30m"
        );
        assert_eq!(
            relative_expiry(now, now - chrono::Duration::minutes(5)),
            "expired 5m ago"
        );
    }

    #[test]
    fn missing_expiry_is_a_non_expiring_credit() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
            .single()
            .unwrap();
        let credit = credit(None, Some(BankedResetCreditStatus::Available));
        assert_eq!(expiry_label(&credit, now), "Does not expire");
    }

    fn credit(
        expires_at: Option<DateTime<Utc>>,
        status: Option<BankedResetCreditStatus>,
    ) -> BankedResetCredit {
        BankedResetCredit {
            expires_at,
            title: None,
            status,
        }
    }
}
