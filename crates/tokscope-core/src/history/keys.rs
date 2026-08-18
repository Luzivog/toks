use std::fmt;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::Serialize;

use super::UsagePeriod;

/// Parsed calendar key for a history bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageKey {
    Hourly(NaiveDateTime),
    Daily(NaiveDate),
    Monthly(NaiveDate),
}

impl UsageKey {
    pub fn parse(period: UsagePeriod, raw: &str) -> Option<Self> {
        match period {
            UsagePeriod::Hourly => NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M")
                .ok()
                .map(Self::Hourly),
            UsagePeriod::Daily => NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .ok()
                .map(Self::Daily),
            UsagePeriod::Monthly => NaiveDate::parse_from_str(&format!("{raw}-01"), "%Y-%m-%d")
                .ok()
                .map(Self::Monthly),
        }
    }

    pub const fn period(self) -> UsagePeriod {
        match self {
            Self::Hourly(_) => UsagePeriod::Hourly,
            Self::Daily(_) => UsagePeriod::Daily,
            Self::Monthly(_) => UsagePeriod::Monthly,
        }
    }

    fn shift(self, offset: i64) -> Option<Self> {
        match self {
            Self::Hourly(value) => value
                .checked_add_signed(Duration::hours(offset))
                .map(Self::Hourly),
            Self::Daily(value) => value
                .checked_add_signed(Duration::days(offset))
                .map(Self::Daily),
            Self::Monthly(value) => {
                let index = i64::from(value.year()) * 12 + i64::from(value.month0()) + offset;
                let year = i32::try_from(index.div_euclid(12)).ok()?;
                let month = u32::try_from(index.rem_euclid(12) + 1).ok()?;
                NaiveDate::from_ymd_opt(year, month, 1).map(Self::Monthly)
            }
        }
    }
}

impl fmt::Display for UsageKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hourly(value) => write!(formatter, "{}", value.format("%Y-%m-%d %H:00")),
            Self::Daily(value) => write!(formatter, "{}", value.format("%Y-%m-%d")),
            Self::Monthly(value) => write!(formatter, "{}", value.format("%Y-%m")),
        }
    }
}

/// Inclusive query range whose endpoints always share one granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageRange {
    pub start: UsageKey,
    pub end: UsageKey,
}

impl UsageRange {
    pub fn new(start: UsageKey, end: UsageKey) -> Option<Self> {
        (start.period() == end.period() && start <= end).then_some(Self { start, end })
    }

    pub fn trailing(end: UsageKey, bucket_count: u32) -> Option<Self> {
        let distance = i64::from(bucket_count.max(1)) - 1;
        Self::new(end.shift(-distance)?, end)
    }

    pub const fn period(self) -> UsagePeriod {
        self.start.period()
    }

    pub fn contains(self, key: UsageKey) -> bool {
        key.period() == self.period() && self.start <= key && key <= self.end
    }
}
