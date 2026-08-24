use chrono::{TimeZone, Utc};

use super::format::{fmt_reset, zone_suffix};

#[test]
fn multi_day_reset_countdowns_include_minutes() {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
        .single()
        .unwrap();
    assert_eq!(
        fmt_reset(now, Some(now + chrono::Duration::days(6))),
        "resets in 6d 0h 0m"
    );
    assert_eq!(
        fmt_reset(
            now,
            Some(now + chrono::Duration::days(6) + chrono::Duration::minutes(5))
        ),
        "resets in 6d 0h 5m"
    );
}

#[test]
fn numeric_timezone_is_not_repeated() {
    assert_eq!(zone_suffix("+02:00", "+02:00"), "+02:00");
}

#[test]
fn named_timezone_keeps_its_numeric_offset() {
    assert_eq!(zone_suffix("CEST", "+02:00"), "CEST (+02:00)");
}
