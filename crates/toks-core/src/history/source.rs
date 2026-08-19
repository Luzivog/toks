use chrono::Duration;
use std::collections::HashMap;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::sessions::UnifiedMessage;

use super::ingress::ValidatedMessage;
use super::rollup::UsageRollup;
use super::totals::UsageTotals;
use super::{DaySlice, MinuteSlice, SourceHistory, UsageKey, DAYS_SPAN, MINUTES_SPAN};

#[derive(Default)]
pub(super) struct Accum {
    minutes: HashMap<i64, UsageTotals>,
    usage: UsageRollup,
}

impl Accum {
    pub(super) fn add(&mut self, m: &UnifiedMessage, now_minute: i64, timezone: &BucketTimezone) {
        let message = ValidatedMessage::new(m);

        let minute = message.timestamp() / 60_000;
        if message.timestamp() > 0 && minute <= now_minute && now_minute - minute < MINUTES_SPAN {
            self.minutes.entry(minute).or_default().add(&message);
        }

        self.usage.add(&message, timezone);
    }

    pub(super) fn finish(
        self,
        client: &str,
        now_minute: i64,
        today: chrono::NaiveDate,
    ) -> SourceHistory {
        let minutes: Vec<MinuteSlice> = (0..MINUTES_SPAN)
            .map(|i| {
                let minute = now_minute - (MINUTES_SPAN - 1) + i;
                let bucket = self.minutes.get(&minute);
                MinuteSlice {
                    minute,
                    tokens: bucket.map(UsageTotals::tokens).unwrap_or(0),
                    cost: bucket.map(UsageTotals::cost).unwrap_or(0.0),
                    models: bucket.map(UsageTotals::model_usage).unwrap_or_default(),
                }
            })
            .collect();

        let days: Vec<DaySlice> = (0..DAYS_SPAN)
            .map(|i| {
                let date = (today - Duration::days(DAYS_SPAN - 1 - i))
                    .format("%Y-%m-%d")
                    .to_string();
                let key = UsageKey::Daily(today - Duration::days(DAYS_SPAN - 1 - i));
                let bucket = self.usage.daily.get(&key);
                DaySlice {
                    date,
                    tokens: bucket.map(UsageTotals::tokens).unwrap_or(0),
                    cost: bucket.map(UsageTotals::cost).unwrap_or(0.0),
                    messages: bucket.map(UsageTotals::messages).unwrap_or(0),
                }
            })
            .collect();

        let today_bucket = self.usage.daily.get(&UsageKey::Daily(today));
        let today_tokens = today_bucket.map(UsageTotals::tokens).unwrap_or(0);
        let today_cost = today_bucket.map(UsageTotals::cost).unwrap_or(0.0);
        let week_cost: f64 = (0..7)
            .map(|i| {
                self.usage
                    .daily
                    .get(&UsageKey::Daily(today - Duration::days(i)))
                    .map(UsageTotals::cost)
                    .unwrap_or(0.0)
            })
            .sum();

        let total = self.usage.total.clone();
        let usage = self.usage.finish();
        let mut models = total.model_usage();
        models.sort_by(|a, b| {
            b.cost
                .partial_cmp(&a.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        SourceHistory {
            client: client.to_string(),
            minutes,
            days,
            usage,
            models,
            today_tokens,
            today_cost,
            week_cost,
            total_tokens: total.tokens(),
            total_cost: total.cost(),
            total_messages: total.messages(),
            cost_coverage: total.coverage(),
        }
    }
}
