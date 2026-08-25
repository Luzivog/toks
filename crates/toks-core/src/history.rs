//! Usage history built on Toks's scan, parse, deduplication, and pricing
//! pipeline, then reshaped into chart-ready series.
//!
//! [`LocalHistory`] is the deep module used by new callers. It owns discovery,
//! durable hydration, refresh, pricing, and last-good fallback behind three
//! entry points. [`collect`] and [`hydrate`] remain compatibility shims while
//! the desktop app migrates to that interface.

mod archive;
mod cache;
mod collect;
mod hydration;
#[cfg(test)]
mod hydration_tests;
#[cfg(test)]
mod ingress;
mod keys;
mod local_history;
#[cfg(test)]
mod materialize;
#[cfg(test)]
mod rollup;
mod selection;
#[cfg(test)]
mod source;
#[cfg(test)]
mod totals;
mod types;
mod validation;

pub use collect::collect;
pub use hydration::{hydrate, HistoryHydration};
pub use keys::{UsageKey, UsageRange};
pub use local_history::{CatchUpRetry, HistoryStatus, HistoryView, LocalHistory};
pub use selection::merge_source_usage;
pub use types::{
    CostCoverage, DaySlice, HistorySnapshot, MinuteSlice, ModelRow, ModelUsage, SourceHistory,
    UsageBucket, UsagePeriod, UsageSeries,
};

/// Permanently exclude an already captured local-time range from Toks.
/// Provider transcript files are never modified.
pub fn forget_range(start_ms: i64, end_ms: i64) -> anyhow::Result<usize> {
    archive::forget_range_default(start_ms, end_ms)
}

/// Provider clients included in the initial product surface.
const CLIENTS: &[&str] = &["claude", "codex", "opencode"];
/// Live-throughput window, in minutes.
pub const MINUTES_SPAN: i64 = 60;
/// Overview-history window, in days.
pub const DAYS_SPAN: i64 = 30;

/// Human time label for a unix-minute value, in local time (`HH:MM`).
pub fn minute_label(minute: i64) -> String {
    use chrono::TimeZone;

    chrono::Local
        .timestamp_opt(minute * 60, 0)
        .single()
        .map(|time| time.format("%H:%M").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod materialize_tests;
#[cfg(test)]
mod tests;
