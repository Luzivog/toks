use chrono::Datelike;
use std::collections::BTreeMap;
use tokscope_ingest::bucket_tz::BucketTimezone;

use super::ingress::ValidatedMessage;
use super::totals::UsageTotals;
use super::{SourceHistory, UsageBucket, UsageKey, UsagePeriod, UsageSeries};

pub(super) fn merge_usage_series(sources: &[SourceHistory]) -> UsageSeries {
    fn merge<'a>(
        period: UsagePeriod,
        buckets: impl IntoIterator<Item = &'a UsageBucket>,
    ) -> Vec<UsageBucket> {
        let mut totals: BTreeMap<UsageKey, UsageTotals> = BTreeMap::new();
        for bucket in buckets {
            if let Some(key) = UsageKey::parse(period, &bucket.key) {
                totals.entry(key).or_default().add_bucket(bucket);
            }
        }
        totals
            .into_iter()
            .map(|(key, total)| total.into_bucket(key.to_string()))
            .collect()
    }

    UsageSeries {
        daily: merge(
            UsagePeriod::Daily,
            sources.iter().flat_map(|source| &source.usage.daily),
        ),
        hourly: merge(
            UsagePeriod::Hourly,
            sources.iter().flat_map(|source| &source.usage.hourly),
        ),
        monthly: merge(
            UsagePeriod::Monthly,
            sources.iter().flat_map(|source| &source.usage.monthly),
        ),
    }
}

#[derive(Default)]
pub(super) struct UsageRollup {
    pub(super) total: UsageTotals,
    pub(super) daily: BTreeMap<UsageKey, UsageTotals>,
    hourly: BTreeMap<UsageKey, UsageTotals>,
    monthly: BTreeMap<UsageKey, UsageTotals>,
}

impl UsageRollup {
    pub(super) fn add(&mut self, message: &ValidatedMessage<'_>, timezone: &BucketTimezone) {
        self.total.add(message);

        let day = UsageKey::parse(UsagePeriod::Daily, message.date());
        if let Some(day) = day {
            self.daily.entry(day).or_default().add(message);
        }

        let midnight = day.and_then(|key| match key {
            UsageKey::Daily(date) => date.and_hms_opt(0, 0, 0).map(UsageKey::Hourly),
            _ => None,
        });
        let hour = if message.timestamp() > 0 {
            timezone
                .hour_key(message.timestamp())
                .and_then(|key| UsageKey::parse(UsagePeriod::Hourly, &key))
                .or(midnight)
        } else {
            midnight
        };
        if let Some(hour) = hour {
            self.hourly.entry(hour).or_default().add(message);
        }

        if let Some(UsageKey::Daily(date)) = day {
            self.monthly
                .entry(UsageKey::Monthly(
                    date.with_day(1).expect("a valid date has a first day"),
                ))
                .or_default()
                .add(message);
        }
    }

    pub(super) fn finish(self) -> UsageSeries {
        fn buckets(entries: BTreeMap<UsageKey, UsageTotals>) -> Vec<UsageBucket> {
            entries
                .into_iter()
                .map(|(key, totals)| totals.into_bucket(key.to_string()))
                .collect()
        }

        UsageSeries {
            daily: buckets(self.daily),
            hourly: buckets(self.hourly),
            monthly: buckets(self.monthly),
        }
    }
}
