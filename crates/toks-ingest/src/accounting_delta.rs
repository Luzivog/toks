//! Source-checkpointed accounting changes for durable local history.

mod claude;
mod codex;
mod collector;
mod fingerprint;
mod identity;
mod lock;
mod opencode;
#[cfg(test)]
mod opencode_tests;
mod store;
mod types;

pub use collector::AccountingDeltaCollector;
pub use types::{
    AccountingBacklog, AccountingDelta, AccountingDeltaOptions, SourceCheckpoint, SourceDelta,
    SourceKey, SourceRevision,
};

/// Stable error value returned when another process owns the checkpoint writer.
pub const COLLECTOR_BUSY_ERROR: &str = "accounting checkpoint writer is busy";

#[cfg(test)]
mod tests;
