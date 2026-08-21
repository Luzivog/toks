use std::collections::BTreeMap;

use chrono::Datelike;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::PricingService;

use super::totals::Totals;
use crate::history::archive::ArchiveRollup;
use crate::history::{UsageBucket, UsageKey, UsagePeriod, UsageSeries};

#[derive(Default)]
pub(super) struct GlobalProjection {
    daily: BTreeMap<UsageKey, Totals>,
    hourly: BTreeMap<UsageKey, Totals>,
    monthly: BTreeMap<UsageKey, Totals>,
}

impl GlobalProjection {
    pub(super) fn add(
        &mut self,
        row: &ArchiveRollup,
        timezone: &BucketTimezone,
        pricing: Option<&PricingService>,
    ) {
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

    pub(super) fn finish(self) -> UsageSeries {
        UsageSeries {
            daily: buckets(self.daily),
            hourly: buckets(self.hourly),
            monthly: buckets(self.monthly),
        }
    }
}

pub(super) fn buckets(entries: BTreeMap<UsageKey, Totals>) -> Vec<UsageBucket> {
    entries
        .into_iter()
        .map(|(key, total)| total.into_bucket(key.to_string()))
        .collect()
}

pub(super) fn canonical_client(client: &str) -> Option<&'static str> {
    match client {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        value if value.starts_with("cc-mirror/") => Some("claude"),
        _ => None,
    }
}
