use std::collections::HashMap;

use chrono::Utc;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::PricingService;

use super::{merge, time, ClientProjection};
use crate::history::archive::{ArchiveProjection, ArchiveRollup, RollupPeriod};
use crate::history::{CostCoverage, HistorySnapshot, CLIENTS};

pub(in crate::history) struct ProjectionBuilder<'a> {
    clients: HashMap<&'static str, ClientProjection>,
    global: merge::GlobalProjection,
    now: chrono::DateTime<Utc>,
    now_minute: i64,
    today: chrono::NaiveDate,
    timezone: &'a BucketTimezone,
    pricing: Option<&'a PricingService>,
}

impl<'a> ProjectionBuilder<'a> {
    pub(in crate::history) fn new(
        now: chrono::DateTime<Utc>,
        timezone: &'a BucketTimezone,
        pricing: Option<&'a PricingService>,
    ) -> Self {
        Self {
            clients: HashMap::new(),
            global: merge::GlobalProjection::default(),
            now,
            now_minute: now.timestamp().div_euclid(60),
            today: time::current_day(now, timezone),
            timezone,
            pricing,
        }
    }

    pub(in crate::history) fn add(&mut self, row: &ArchiveRollup) {
        let Some(client) = merge::canonical_client(&row.client) else {
            return;
        };
        if row.period == RollupPeriod::Minute {
            self.global.add(row, self.timezone, self.pricing);
        }
        self.clients.entry(client).or_default().add(
            row,
            self.timezone,
            self.pricing,
            self.now_minute,
        );
    }

    pub(in crate::history) fn finish(mut self, projection: ArchiveProjection) -> HistorySnapshot {
        let mut sources: Vec<_> = CLIENTS
            .iter()
            .filter_map(|client| {
                self.clients
                    .remove(*client)
                    .map(|usage| usage.finish(client, self.now_minute, self.today))
            })
            .collect();
        sources.sort_by(|left, right| left.client.cmp(&right.client));
        let mut cost_coverage = CostCoverage::default();
        for source in &sources {
            cost_coverage.add_assign(source.cost_coverage);
        }
        HistorySnapshot {
            sources,
            usage: self.global.finish(),
            generated_at_ms: self.now.timestamp_millis(),
            captured_since_ms: projection.captured_since_ms,
            captured_through_ms: projection.captured_through_ms,
            strong_events: projection.strong_events,
            weak_events: projection.weak_events,
            history_conflicts: projection.conflicts,
            cost_coverage,
            unpriced: !cost_coverage.is_complete(),
        }
    }
}

#[cfg(test)]
pub(super) fn snapshot(
    mut projection: ArchiveProjection,
    now: chrono::DateTime<Utc>,
    timezone: &BucketTimezone,
    pricing: Option<&PricingService>,
) -> HistorySnapshot {
    let rollups = std::mem::take(&mut projection.rollups);
    let mut builder = ProjectionBuilder::new(now, timezone, pricing);
    for row in &rollups {
        builder.add(row);
    }
    builder.finish(projection)
}
