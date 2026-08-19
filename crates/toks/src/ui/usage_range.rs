use chrono::NaiveDate;
use toks_core::history::{UsageBucket, UsagePeriod, UsageSeries};

use crate::{SortDirection, SortState, UsageSortColumn};

use super::cost_per_million;

pub(super) fn usage_period_label(period: UsagePeriod) -> &'static str {
    match period {
        UsagePeriod::Daily => "Daily",
        UsagePeriod::Hourly => "Hourly",
        UsagePeriod::Monthly => "Monthly",
    }
}

pub(super) fn usage_bucket_label(period: UsagePeriod, key: &str) -> String {
    match period {
        UsagePeriod::Daily => chrono::NaiveDate::parse_from_str(key, "%Y-%m-%d")
            .map(|date| date.format("%b %-d, %Y").to_string())
            .unwrap_or_else(|_| key.to_string()),
        UsagePeriod::Hourly => chrono::NaiveDateTime::parse_from_str(key, "%Y-%m-%d %H:%M")
            .map(|time| time.format("%H:%M").to_string())
            .unwrap_or_else(|_| key.to_string()),
        UsagePeriod::Monthly => chrono::NaiveDate::parse_from_str(&format!("{key}-01"), "%Y-%m-%d")
            .map(|date| date.format("%B %Y").to_string())
            .unwrap_or_else(|_| key.to_string()),
    }
}

pub(super) fn hourly_bucket_day(key: &str) -> Option<NaiveDate> {
    chrono::NaiveDateTime::parse_from_str(key, "%Y-%m-%d %H:%M")
        .ok()
        .map(|time| time.date())
}

pub(super) fn hourly_bucket_full_label(key: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(key, "%Y-%m-%d %H:%M")
        .map(|time| time.format("%b %-d · %H:%M").to_string())
        .unwrap_or_else(|_| key.to_string())
}

pub(super) fn visible_usage_buckets(usage: &UsageSeries, period: UsagePeriod) -> Vec<&UsageBucket> {
    usage
        .buckets(period)
        .iter()
        .rev()
        .filter(|bucket| {
            bucket.tokens > 0 || bucket.messages > 0 || bucket.turns > 0 || bucket.cost > 0.0
        })
        .collect()
}

pub(super) fn usage_range_label() -> &'static str {
    "All history"
}

pub(super) fn sort_usage_buckets(buckets: &mut [&UsageBucket], sort: SortState<UsageSortColumn>) {
    let Some(column) = sort.column else {
        return;
    };
    buckets.sort_by(|a, b| {
        let order = match column {
            UsageSortColumn::Period => a.key.cmp(&b.key),
            UsageSortColumn::Turns => a.turns.cmp(&b.turns),
            UsageSortColumn::Messages => a.messages.cmp(&b.messages),
            UsageSortColumn::Input => a.input.cmp(&b.input),
            UsageSortColumn::Output => a.output.cmp(&b.output),
            UsageSortColumn::Reasoning => a.reasoning.cmp(&b.reasoning),
            UsageSortColumn::CacheRead => a.cache_read.cmp(&b.cache_read),
            UsageSortColumn::CacheWrite => a.cache_write.cmp(&b.cache_write),
            UsageSortColumn::Total => a.tokens.cmp(&b.tokens),
            UsageSortColumn::Cost => a.cost.total_cmp(&b.cost),
            UsageSortColumn::CostPerMillion => cost_per_million(a.cost, a.tokens)
                .unwrap_or_default()
                .total_cmp(&cost_per_million(b.cost, b.tokens).unwrap_or_default()),
        };
        match sort.direction {
            SortDirection::Ascending => order,
            SortDirection::Descending => order.reverse(),
        }
        .then_with(|| b.key.cmp(&a.key))
    });
}

pub(super) fn usage_bucket_is_current(period: UsagePeriod, key: &str, latest: &str) -> bool {
    matches!(period, UsagePeriod::Daily | UsagePeriod::Monthly) && key == latest
}
