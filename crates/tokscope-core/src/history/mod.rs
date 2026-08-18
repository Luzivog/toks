//! Usage history built on Tokscope's scan, parse, deduplication, and pricing
//! pipeline, then reshaped into chart-ready series.
//!
//! [`collect`] is the single interface used by the app. Its implementation
//! reuses the ingest cache, performs one scan, and derives every period from
//! the same parsed messages.

mod cache;
mod collect;
mod ingress;
mod keys;
mod rollup;
mod source;
mod totals;
mod types;
mod validation;

pub use collect::collect;
pub use keys::{UsageKey, UsageRange};
pub use source::minute_label;
pub use types::{
    CostCoverage, DaySlice, HistorySnapshot, MinuteSlice, ModelRow, ModelUsage, SourceHistory,
    UsageBucket, UsagePeriod, UsageSeries,
};

/// Provider clients included in the initial product surface.
const CLIENTS: &[&str] = &["claude", "codex"];
/// Live-throughput window, in minutes.
pub const MINUTES_SPAN: i64 = 60;
/// Overview-history window, in days.
pub const DAYS_SPAN: i64 = 30;

/// Loads the last successfully collected aggregate without scanning providers.
/// Corrupt, incompatible, or structurally invalid snapshots are ignored.
pub fn hydrate() -> Option<HistorySnapshot> {
    cache::load()
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod tests;
