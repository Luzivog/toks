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
    AccountingAdvance, AccountingAdvanceError, AccountingBacklog, AccountingDeltaOptions,
    AccountingSource, SourceKey, SourceRevision,
};

/// Stable error value returned when another process owns the checkpoint writer.
pub const COLLECTOR_BUSY_ERROR: &str = "accounting checkpoint writer is busy";

#[cfg(test)]
mod tests;
#[cfg(test)]
mod version_tests;
