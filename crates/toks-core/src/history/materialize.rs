use std::collections::HashMap;

use chrono::Utc;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::PricingService;
use toks_ingest::sessions::{CostSource, UnifiedMessage};

use super::archive::ArchiveCapture;
use super::rollup::merge_usage_series;
use super::source::Accum;
use super::{CostCoverage, HistorySnapshot, SourceHistory, UsageKey, UsagePeriod, CLIENTS};

pub(super) fn snapshot(
    mut capture: ArchiveCapture,
    now: chrono::DateTime<Utc>,
    timezone: &BucketTimezone,
    pricing: Option<&PricingService>,
) -> HistorySnapshot {
    prepare_messages(&mut capture.messages, timezone, pricing);
    let now_minute = now.timestamp() / 60;
    let today = current_day(now, timezone);
    let mut per_client: HashMap<&str, Accum> = HashMap::new();
    for message in &capture.messages {
        let Some(client) = canonical_client(&message.client) else {
            continue;
        };
        per_client
            .entry(client)
            .or_default()
            .add(message, now_minute, timezone);
    }
    let mut sources: Vec<SourceHistory> = CLIENTS
        .iter()
        .filter_map(|client| {
            per_client
                .remove(*client)
                .map(|usage| usage.finish(client, now_minute, today))
        })
        .collect();
    sources.sort_by(|left, right| left.client.cmp(&right.client));
    let usage = merge_usage_series(&sources);
    let mut cost_coverage = CostCoverage::default();
    for source in &sources {
        cost_coverage.add_assign(source.cost_coverage);
    }
    HistorySnapshot {
        sources,
        usage,
        generated_at_ms: now.timestamp_millis(),
        captured_since_ms: capture.captured_since_ms,
        captured_through_ms: capture.captured_through_ms,
        strong_events: capture.strong_events,
        weak_events: capture.weak_events,
        history_conflicts: capture.conflicts,
        cost_coverage,
        unpriced: !cost_coverage.is_complete(),
    }
}

fn canonical_client(client: &str) -> Option<&'static str> {
    match client {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        client if client.starts_with("cc-mirror/") => Some("claude"),
        _ => None,
    }
}

fn prepare_messages(
    messages: &mut [UnifiedMessage],
    timezone: &BucketTimezone,
    pricing: Option<&PricingService>,
) {
    for message in messages {
        message.date = if message.timestamp > 0 {
            timezone.day_key(message.timestamp)
        } else {
            String::new()
        };
        if message.cost_source == CostSource::ProviderReported {
            continue;
        }
        let Some(pricing) = pricing else {
            message.cost = 0.0;
            message.cost_source = CostSource::Unknown;
            continue;
        };
        let provider = (!message.provider_id.is_empty()).then_some(message.provider_id.as_str());
        message.cost =
            pricing.calculate_cost_with_provider(&message.model_id, provider, &message.tokens);
        message.cost_source =
            if pricing.covers_usage_with_provider(&message.model_id, provider, &message.tokens) {
                CostSource::Estimated
            } else {
                CostSource::Unknown
            };
    }
}

fn current_day(now: chrono::DateTime<Utc>, timezone: &BucketTimezone) -> chrono::NaiveDate {
    UsageKey::parse(
        UsagePeriod::Daily,
        &timezone.day_key(now.timestamp_millis()),
    )
    .and_then(|key| match key {
        UsageKey::Daily(date) => Some(date),
        _ => None,
    })
    .unwrap_or_else(|| now.with_timezone(&chrono::Local).date_naive())
}
