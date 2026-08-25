use chrono::{Datelike, Duration, NaiveDate};
use toks_core::history::{HistorySnapshot, SourceHistory, UsageKey, UsagePeriod};
use toks_core::{ClientId, ProviderVisibility};

use super::{
    provider_point, visible_source, visible_usage, week_start, ProviderPoint, UsageSummary,
};

pub(super) fn all_time_points(
    history: &HistorySnapshot,
    visibility: &ProviderVisibility,
) -> Vec<ProviderPoint> {
    let usage = visible_usage(history, visibility);
    let mut active = usage
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
            source_week_values(visible_source(history, ClientId::Claude, visibility), week),
            source_week_values(visible_source(history, ClientId::Codex, visibility), week),
            source_week_values(
                visible_source(history, ClientId::OpenCode, visibility),
                week,
            ),
        )
    })
    .collect()
}

pub(super) fn all_time_summary(
    history: &HistorySnapshot,
    visibility: &ProviderVisibility,
) -> UsageSummary {
    let totals = |provider| {
        visible_source(history, provider, visibility)
            .map(|source| (source.total_cost.max(0.0), source.total_tokens.max(0)))
            .unwrap_or_default()
    };
    let (claude_cost, claude_tokens) = totals(ClientId::Claude);
    let (codex_cost, codex_tokens) = totals(ClientId::Codex);
    let (opencode_cost, opencode_tokens) = totals(ClientId::OpenCode);
    UsageSummary::exact(
        claude_cost,
        claude_tokens,
        codex_cost,
        codex_tokens,
        opencode_cost,
        opencode_tokens,
    )
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
