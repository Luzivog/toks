use chrono::{Datelike, Duration, NaiveDate};
use toks_core::history::{HistorySnapshot, SourceHistory, UsageKey, UsagePeriod};

use super::{provider_point, ProviderPoint, UsageSummary};

pub(super) fn all_time_points(history: &HistorySnapshot) -> Vec<ProviderPoint> {
    let mut active = history
        .usage
        .daily
        .iter()
        .filter(|bucket| bucket.tokens > 0 || bucket.cost > 0.0)
        .filter_map(|bucket| match bucket.typed_key(UsagePeriod::Daily) {
            Some(UsageKey::Daily(date)) => Some(week_start(date)),
            _ => None,
        });
    let Some(first) = active.next() else {
        return Vec::new();
    };
    let (first, last) = active.fold((first, first), |(first, last), week| {
        (first.min(week), last.max(week))
    });

    std::iter::successors(Some(first), |week| {
        week.checked_add_signed(Duration::weeks(1))
    })
    .take_while(|week| *week <= last)
    .map(|week| {
        provider_point(
            week_heading(week),
            week.format("%b %-d").to_string(),
            source_week_values(history.source("claude"), week),
            source_week_values(history.source("codex"), week),
        )
    })
    .collect()
}

pub(super) fn all_time_summary(history: &HistorySnapshot) -> UsageSummary {
    let totals = |client: &str| {
        history
            .source(client)
            .map(|source| (source.total_cost.max(0.0), source.total_tokens.max(0)))
            .unwrap_or_default()
    };
    let (claude_cost, claude_tokens) = totals("claude");
    let (codex_cost, codex_tokens) = totals("codex");
    UsageSummary::exact(claude_cost, claude_tokens, codex_cost, codex_tokens)
}

fn source_week_values(source: Option<&SourceHistory>, week: NaiveDate) -> (f64, i64) {
    let end = week + Duration::weeks(1);
    source
        .into_iter()
        .flat_map(|source| &source.usage.daily)
        .filter(|bucket| {
            matches!(
                bucket.typed_key(UsagePeriod::Daily),
                Some(UsageKey::Daily(date)) if date >= week && date < end
            )
        })
        .fold((0.0, 0_i64), |(cost, tokens), bucket| {
            (
                cost + bucket.cost.max(0.0),
                tokens.saturating_add(bucket.tokens.max(0)),
            )
        })
}

fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

fn week_heading(start: NaiveDate) -> String {
    let end = start + Duration::days(6);
    if start.year() != end.year() {
        format!(
            "{}–{}",
            start.format("%B %-d, %Y"),
            end.format("%B %-d, %Y")
        )
    } else if start.month() != end.month() {
        format!("{}–{}", start.format("%B %-d"), end.format("%B %-d, %Y"))
    } else {
        format!("{}–{}, {}", start.format("%B %-d"), end.day(), end.year())
    }
}
