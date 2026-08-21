mod builder;
mod merge;
mod time;
mod totals;

use std::collections::{BTreeMap, HashMap};

use chrono::Duration;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::PricingService;

use self::totals::Totals;
use crate::history::archive::{ArchiveRollup, RollupPeriod};
use crate::history::{
    DaySlice, MinuteSlice, SourceHistory, UsageKey, UsagePeriod, UsageSeries, DAYS_SPAN,
    MINUTES_SPAN,
};
#[cfg(test)]
use builder::snapshot;
pub(super) use builder::ProjectionBuilder;

#[derive(Default)]
struct ClientProjection {
    total: Totals,
    minutes: HashMap<i64, Totals>,
    daily: BTreeMap<UsageKey, Totals>,
    hourly: BTreeMap<UsageKey, Totals>,
}

impl ClientProjection {
    fn add(
        &mut self,
        row: &ArchiveRollup,
        timezone: &BucketTimezone,
        pricing: Option<&PricingService>,
        now_minute: i64,
        today: chrono::NaiveDate,
    ) {
        match row.period {
            RollupPeriod::All => self.total.add(row, pricing),
            RollupPeriod::Minute => self.add_minute(row, timezone, pricing, now_minute, today),
        }
    }

    fn add_minute(
        &mut self,
        row: &ArchiveRollup,
        timezone: &BucketTimezone,
        pricing: Option<&PricingService>,
        now_minute: i64,
        today: chrono::NaiveDate,
    ) {
        let minute = row.bucket_start_ms.div_euclid(60_000);
        if (now_minute - (MINUTES_SPAN - 1)..=now_minute).contains(&minute) {
            self.minutes.entry(minute).or_default().add(row, pricing);
        }
        let Some(day) = UsageKey::parse(UsagePeriod::Daily, &timezone.day_key(row.bucket_start_ms))
        else {
            return;
        };
        self.daily.entry(day).or_default().add(row, pricing);
        if day == UsageKey::Daily(today) {
            if let Some(hour) = timezone
                .hour_key(row.bucket_start_ms)
                .and_then(|key| UsageKey::parse(UsagePeriod::Hourly, &key))
            {
                self.hourly.entry(hour).or_default().add(row, pricing);
            }
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
            daily: merge::buckets(self.daily),
            hourly: merge::buckets(self.hourly),
            monthly: Vec::new(),
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

#[cfg(test)]
mod tests;
