mod merge;
mod time;
mod totals;

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, Utc};
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::PricingService;

use self::totals::Totals;
use crate::history::archive::{ArchiveProjection, ArchiveRollup, RollupPeriod};
use crate::history::{
    CostCoverage, DaySlice, HistorySnapshot, MinuteSlice, SourceHistory, UsageKey, UsagePeriod,
    UsageSeries, CLIENTS, DAYS_SPAN, MINUTES_SPAN,
};

#[derive(Default)]
struct ClientProjection {
    total: Totals,
    minutes: HashMap<i64, Totals>,
    daily: BTreeMap<UsageKey, Totals>,
    hourly: BTreeMap<UsageKey, Totals>,
    monthly: BTreeMap<UsageKey, Totals>,
}

impl ClientProjection {
    fn add(
        &mut self,
        row: &ArchiveRollup,
        timezone: &BucketTimezone,
        pricing: Option<&PricingService>,
    ) {
        match row.period {
            RollupPeriod::All => self.total.add(row, pricing),
            RollupPeriod::Minute => self.add_minute(row, timezone, pricing),
        }
    }

    fn add_minute(
        &mut self,
        row: &ArchiveRollup,
        timezone: &BucketTimezone,
        pricing: Option<&PricingService>,
    ) {
        let minute = row.bucket_start_ms.div_euclid(60_000);
        self.minutes.entry(minute).or_default().add(row, pricing);
        let Some(day) = UsageKey::parse(UsagePeriod::Daily, &timezone.day_key(row.bucket_start_ms))
        else {
            return;
        };
        self.daily.entry(day).or_default().add(row, pricing);
        if let Some(hour) = timezone
            .hour_key(row.bucket_start_ms)
            .and_then(|key| UsageKey::parse(UsagePeriod::Hourly, &key))
        {
            self.hourly.entry(hour).or_default().add(row, pricing);
        }
        if let UsageKey::Daily(date) = day {
            let month = date.with_day(1).expect("a valid date has a first day");
            self.monthly
                .entry(UsageKey::Monthly(month))
                .or_default()
                .add(row, pricing);
        }
    }

    fn finish(self, client: &str, now_minute: i64, today: chrono::NaiveDate) -> SourceHistory {
        let minutes = (0..MINUTES_SPAN)
            .map(|offset| {
                let minute = now_minute - (MINUTES_SPAN - 1) + offset;
                let totals = self.minutes.get(&minute);
                MinuteSlice {
                    minute,
                    tokens: totals.map(Totals::tokens).unwrap_or(0),
                    cost: totals.map(Totals::cost).unwrap_or(0.0),
                    models: totals.map(Totals::model_usage).unwrap_or_default(),
                }
            })
            .collect();
        let days = (0..DAYS_SPAN)
            .map(|offset| {
                let date = today - Duration::days(DAYS_SPAN - 1 - offset);
                let totals = self.daily.get(&UsageKey::Daily(date));
                DaySlice {
                    date: date.format("%Y-%m-%d").to_string(),
                    tokens: totals.map(Totals::tokens).unwrap_or(0),
                    cost: totals.map(Totals::cost).unwrap_or(0.0),
                    messages: totals.map(Totals::messages).unwrap_or(0),
                }
            })
            .collect();
        let today_totals = self.daily.get(&UsageKey::Daily(today));
        let today_tokens = today_totals.map(Totals::tokens).unwrap_or(0);
        let today_cost = today_totals.map(Totals::cost).unwrap_or(0.0);
        let week_cost = (0..7)
            .filter_map(|days| {
                self.daily
                    .get(&UsageKey::Daily(today - Duration::days(days)))
            })
            .map(Totals::cost)
            .sum();
        let usage = UsageSeries {
            daily: buckets(self.daily),
            hourly: buckets(self.hourly),
            monthly: buckets(self.monthly),
        };
        let mut models = self.total.model_usage();
        models.sort_by(|left, right| {
            right
                .cost
                .partial_cmp(&left.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        SourceHistory {
            client: client.into(),
            minutes,
            days,
            usage,
            models,
            today_tokens,
            today_cost,
            week_cost,
            total_tokens: self.total.tokens(),
            total_cost: self.total.cost(),
            total_messages: self.total.messages(),
            cost_coverage: self.total.coverage(),
        }
    }
}

pub(super) fn snapshot(
    projection: ArchiveProjection,
    now: chrono::DateTime<Utc>,
    timezone: &BucketTimezone,
    pricing: Option<&PricingService>,
) -> HistorySnapshot {
    let mut clients: HashMap<&'static str, ClientProjection> = HashMap::new();
    for row in &projection.rollups {
        let Some(client) = canonical_client(&row.client) else {
            continue;
        };
        clients
            .entry(client)
            .or_default()
            .add(row, timezone, pricing);
    }
    let today = time::current_day(now, timezone);
    let now_minute = now.timestamp().div_euclid(60);
    let mut sources: Vec<_> = CLIENTS
        .iter()
        .filter_map(|client| {
            clients
                .remove(*client)
                .map(|usage| usage.finish(client, now_minute, today))
        })
        .collect();
    sources.sort_by(|left, right| left.client.cmp(&right.client));
    let usage = merge::usage_series(&sources);
    let mut cost_coverage = CostCoverage::default();
    for source in &sources {
        cost_coverage.add_assign(source.cost_coverage);
    }
    HistorySnapshot {
        sources,
        usage,
        generated_at_ms: now.timestamp_millis(),
        captured_since_ms: projection.captured_since_ms,
        captured_through_ms: projection.captured_through_ms,
        strong_events: projection.strong_events,
        weak_events: projection.weak_events,
        history_conflicts: projection.conflicts,
        cost_coverage,
        unpriced: !cost_coverage.is_complete(),
    }
}

fn buckets(entries: BTreeMap<UsageKey, Totals>) -> Vec<crate::history::UsageBucket> {
    entries
        .into_iter()
        .map(|(key, totals)| totals.into_bucket(key.to_string()))
        .collect()
}

fn canonical_client(client: &str) -> Option<&'static str> {
    match client {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        value if value.starts_with("cc-mirror/") => Some("claude"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
