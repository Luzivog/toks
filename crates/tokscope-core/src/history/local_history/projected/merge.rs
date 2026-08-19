use std::collections::BTreeMap;

use super::totals::Totals;
use crate::history::{SourceHistory, UsageBucket, UsageKey, UsagePeriod, UsageSeries};

pub(super) fn usage_series(sources: &[SourceHistory]) -> UsageSeries {
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

fn merge<'a>(
    period: UsagePeriod,
    buckets: impl IntoIterator<Item = &'a UsageBucket>,
) -> Vec<UsageBucket> {
    let mut totals: BTreeMap<UsageKey, Totals> = BTreeMap::new();
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
