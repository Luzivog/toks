use std::collections::HashSet;

use anyhow::{bail, Result};
use chrono::NaiveDate;

use super::{
    CostCoverage, HistorySnapshot, ModelUsage, SourceHistory, UsageBucket, UsagePeriod,
    UsageSeries, CLIENTS,
};

pub(super) fn validate(snapshot: &HistorySnapshot) -> Result<()> {
    if snapshot.generated_at_ms < 0
        || !capture_valid(snapshot)
        || !coverage_valid(snapshot.cost_coverage)
    {
        bail!("invalid history snapshot metadata");
    }
    let mut clients = HashSet::new();
    for source in &snapshot.sources {
        if !CLIENTS.contains(&source.client.as_str()) || !clients.insert(&source.client) {
            bail!("invalid or duplicate history source");
        }
        validate_source(source)?;
    }
    if !series_valid(&snapshot.usage) {
        bail!("invalid aggregate usage series");
    }
    Ok(())
}

fn capture_valid(snapshot: &HistorySnapshot) -> bool {
    let timestamps_valid = snapshot.captured_since_ms.is_none_or(|value| value >= 0)
        && snapshot.captured_through_ms.is_none_or(|value| value >= 0)
        && match (snapshot.captured_since_ms, snapshot.captured_through_ms) {
            (Some(since), Some(through)) => since <= through,
            (Some(_), None) => false,
            _ => true,
        };
    let total = snapshot.strong_events.saturating_add(snapshot.weak_events);
    timestamps_valid
        && snapshot.strong_events >= 0
        && snapshot.weak_events >= 0
        && snapshot.history_conflicts >= 0
        && snapshot.history_conflicts <= total
}

fn validate_source(source: &SourceHistory) -> Result<()> {
    let metrics_valid = metric(source.total_tokens, source.total_cost)
        && metric(source.today_tokens, source.today_cost)
        && metric(source.total_messages, source.week_cost)
        && coverage_valid(source.cost_coverage)
        && source.models.iter().all(model_valid)
        && source
            .minutes
            .windows(2)
            .all(|pair| pair[0].minute < pair[1].minute)
        && source.minutes.iter().all(|slice| {
            slice.minute >= 0
                && metric(slice.tokens, slice.cost)
                && slice.models.iter().all(model_valid)
        })
        && source
            .days
            .windows(2)
            .all(|pair| pair[0].date < pair[1].date)
        && source.days.iter().all(|slice| {
            NaiveDate::parse_from_str(&slice.date, "%Y-%m-%d").is_ok()
                && metric(slice.tokens, slice.cost)
                && metric(slice.messages, 0.0)
        })
        && series_valid(&source.usage);
    if !metrics_valid {
        bail!("invalid history source metrics");
    }
    Ok(())
}

fn series_valid(series: &UsageSeries) -> bool {
    [
        (UsagePeriod::Daily, &series.daily),
        (UsagePeriod::Hourly, &series.hourly),
        (UsagePeriod::Monthly, &series.monthly),
    ]
    .into_iter()
    .all(|(period, buckets)| ordered_buckets(period, buckets))
}

fn ordered_buckets(period: UsagePeriod, buckets: &[UsageBucket]) -> bool {
    let mut previous = None;
    buckets.iter().all(|bucket| {
        let key = bucket.typed_key(period);
        let ordered = key.is_some() && previous.as_ref().is_none_or(|old| key.as_ref() > Some(old));
        previous = key;
        ordered && bucket_valid(bucket)
    })
}

fn bucket_valid(bucket: &UsageBucket) -> bool {
    metric(bucket.tokens, bucket.cost)
        && metric(bucket.input, 0.0)
        && metric(bucket.output, 0.0)
        && metric(bucket.cache_read, 0.0)
        && metric(bucket.cache_write, 0.0)
        && metric(bucket.reasoning, 0.0)
        && metric(bucket.messages, 0.0)
        && metric(bucket.turns, 0.0)
        && bucket.models.iter().all(model_valid)
        && coverage_valid(bucket.cost_coverage)
}

fn model_valid(model: &ModelUsage) -> bool {
    !model.model.is_empty()
        && !model.provider.is_empty()
        && metric(model.tokens, model.cost)
        && metric(model.input, 0.0)
        && metric(model.output, 0.0)
        && metric(model.cache_read, 0.0)
        && metric(model.cache_write, 0.0)
        && metric(model.reasoning, 0.0)
        && metric(model.messages, 0.0)
        && metric(model.turns, 0.0)
        && coverage_valid(model.cost_coverage)
}

fn metric(value: i64, cost: f64) -> bool {
    value >= 0 && cost.is_finite() && cost >= 0.0
}

fn coverage_valid(value: CostCoverage) -> bool {
    value.covered_tokens >= 0
        && value.uncovered_tokens >= 0
        && value.covered_messages >= 0
        && value.uncovered_messages >= 0
        && value.invalid_records >= 0
}
