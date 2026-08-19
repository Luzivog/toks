use chrono::Utc;
use tokscope_ingest::bucket_tz::BucketTimezone;

use crate::history::{UsageKey, UsagePeriod};

pub(super) fn current_day(
    now: chrono::DateTime<Utc>,
    timezone: &BucketTimezone,
) -> chrono::NaiveDate {
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
