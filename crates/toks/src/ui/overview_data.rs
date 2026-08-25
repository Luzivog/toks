use chrono::{Duration, Local, NaiveDateTime, TimeZone, Timelike};
use toks_core::history::{HistorySnapshot, UsageKey, UsagePeriod};
use toks_core::{ClientId, ProviderVisibility};

use crate::app::OverviewChartRange;

use super::{
    all_time_data::all_time_points, current_usage_date, provider_point, source_bucket_values,
    visible_source, ProviderPoint,
};

pub(super) fn overview_usage_points(
    history: &HistorySnapshot,
    range: OverviewChartRange,
    visibility: &ProviderVisibility,
) -> Vec<ProviderPoint> {
    match range {
        OverviewChartRange::LastTwentyFourHours => trailing_hourly_points(history, visibility),
        OverviewChartRange::LastSevenDays => trailing_daily_points(history, 7, visibility),
        OverviewChartRange::LastThirtyDays => trailing_daily_points(history, 30, visibility),
        OverviewChartRange::AllTime => all_time_points(history, visibility),
    }
}

fn trailing_hourly_points(
    history: &HistorySnapshot,
    visibility: &ProviderVisibility,
) -> Vec<ProviderPoint> {
    let claude = visible_source(history, ClientId::Claude, visibility);
    let codex = visible_source(history, ClientId::Codex, visibility);
    let opencode = visible_source(history, ClientId::OpenCode, visibility);
    let current = current_usage_hour(history);
    (0..24)
        .map(|offset| {
            let hour = current - Duration::hours(23 - offset);
            let key = hour.format("%Y-%m-%d %H:00").to_string();
            provider_point(
                hour.format("%A, %B %-d · %H:00").to_string(),
                hour.format("%H:00").to_string(),
                source_bucket_values(claude, |usage| &usage.hourly, &key),
                source_bucket_values(codex, |usage| &usage.hourly, &key),
                source_bucket_values(opencode, |usage| &usage.hourly, &key),
            )
        })
        .collect()
}

fn trailing_daily_points(
    history: &HistorySnapshot,
    day_count: i64,
    visibility: &ProviderVisibility,
) -> Vec<ProviderPoint> {
    let claude = visible_source(history, ClientId::Claude, visibility);
    let codex = visible_source(history, ClientId::Codex, visibility);
    let opencode = visible_source(history, ClientId::OpenCode, visibility);
    let today = current_usage_date(history);
    (0..day_count)
        .map(|offset| {
            let date = today - Duration::days(day_count - 1 - offset);
            let key = date.format("%Y-%m-%d").to_string();
            provider_point(
                date.format("%A, %B %-d, %Y").to_string(),
                date.format("%m-%d").to_string(),
                source_bucket_values(claude, |usage| &usage.daily, &key),
                source_bucket_values(codex, |usage| &usage.daily, &key),
                source_bucket_values(opencode, |usage| &usage.daily, &key),
            )
        })
        .collect()
}

fn current_usage_hour(history: &HistorySnapshot) -> NaiveDateTime {
    let generated = (history.generated_at_ms > 0)
        .then(|| Local.timestamp_millis_opt(history.generated_at_ms).single())
        .flatten()
        .and_then(|time| time.date_naive().and_hms_opt(time.hour(), 0, 0));
    generated
        .or_else(|| {
            history
                .usage
                .hourly
                .iter()
                .filter_map(|bucket| match bucket.typed_key(UsagePeriod::Hourly) {
                    Some(UsageKey::Hourly(hour)) => Some(hour),
                    _ => None,
                })
                .max()
        })
        .unwrap_or_else(|| {
            current_usage_date(history)
                .and_hms_opt(0, 0, 0)
                .expect("a valid date has a midnight")
        })
}
