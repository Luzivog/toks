use chrono::{DateTime, TimeZone, Utc};
use toks_core::limits::{BankedResetCredit, BankedResetCreditStatus};

use super::banked_reset_tooltip::{expiry_label, relative_expiry, visible_credits};

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
