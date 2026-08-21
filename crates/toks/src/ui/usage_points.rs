use chrono::{Datelike, Local, TimeZone, Timelike};
use toks_core::history::{HistorySnapshot, SourceHistory, UsageBucket, UsagePeriod, UsageSeries};

use super::{current_usage_date, ProviderPoint};

pub(super) fn source_bucket_values(
    source: Option<&SourceHistory>,
    buckets: fn(&UsageSeries) -> &[UsageBucket],
    key: &str,
) -> (f64, i64) {
    source
        .and_then(|source| {
            buckets(&source.usage)
                .iter()
                .find(|bucket| bucket.key == key)
        })
        .map(|bucket| (bucket.cost.max(0.0), bucket.tokens.max(0)))
        .unwrap_or_default()
}

pub(super) fn provider_point(
    heading: String,
    label: String,
    claude: (f64, i64),
    codex: (f64, i64),
    opencode: (f64, i64),
) -> ProviderPoint {
    ProviderPoint {
        heading,
        label: label.into(),
        claude: claude.0,
        claude_tokens: claude.1,
        codex: codex.0,
        codex_tokens: codex.1,
        opencode: opencode.0,
        opencode_tokens: opencode.1,
    }
}

pub(super) fn usage_chart_points(
    history: &HistorySnapshot,
    period: UsagePeriod,
) -> Vec<ProviderPoint> {
    let claude = history.source("claude");
    let codex = history.source("codex");
    let opencode = history.source("opencode");
    let generated_minute = history.generated_at_ms.div_euclid(60_000);

    match period {
        UsagePeriod::Hourly => (0..60)
            .map(|offset| {
                let minute = generated_minute - 59 + offset;
                let local = Local
                    .timestamp_opt(minute * 60, 0)
                    .single()
                    .unwrap_or_else(Local::now);
                let values = |source: Option<&SourceHistory>| {
                    source
                        .and_then(|source| {
                            source.minutes.iter().find(|point| point.minute == minute)
                        })
                        .map(|point| (point.cost.max(0.0), point.tokens.max(0)))
                        .unwrap_or_default()
                };
                provider_point(
                    local.format("%A, %B %-d · %H:%M").to_string(),
                    local.format("%H:%M").to_string(),
                    values(claude),
                    values(codex),
                    values(opencode),
                )
            })
            .collect(),
        UsagePeriod::Daily => {
            let today = current_usage_date(history);
            let generated = Local
                .timestamp_millis_opt(history.generated_at_ms)
                .single()
                .unwrap_or_else(Local::now);
            let visible_hours = if generated.date_naive() == today {
                generated.hour() + 1
            } else if generated.date_naive() > today {
                24
            } else {
                1
            };
            (0..visible_hours)
                .map(|hour| {
                    let key = format!("{} {hour:02}:00", today.format("%Y-%m-%d"));
                    provider_point(
                        format!("{} · {hour:02}:00", today.format("%A, %B %-d")),
                        format!("{hour:02}:00"),
                        source_bucket_values(claude, |usage| &usage.hourly, &key),
                        source_bucket_values(codex, |usage| &usage.hourly, &key),
                        source_bucket_values(opencode, |usage| &usage.hourly, &key),
                    )
                })
                .collect()
        }
        UsagePeriod::Monthly => {
            let today = current_usage_date(history);
            let first = today.with_day(1).unwrap_or(today);
            let day_count = i64::from(today.day());
            (0..day_count)
                .map(|offset| {
                    let date = first + chrono::Duration::days(offset);
                    let key = date.format("%Y-%m-%d").to_string();
                    provider_point(
                        date.format("%A, %B %-d, %Y").to_string(),
                        date.format("%-d").to_string(),
                        source_bucket_values(claude, |usage| &usage.daily, &key),
                        source_bucket_values(codex, |usage| &usage.daily, &key),
                        source_bucket_values(opencode, |usage| &usage.daily, &key),
                    )
                })
                .collect()
        }
    }
}
