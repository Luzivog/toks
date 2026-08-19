use anyhow::{Context, Result};

use super::{HistorySnapshot, LocalHistory};

/// Compatibility shim for callers that only understand a completed snapshot.
pub fn collect() -> Result<HistorySnapshot> {
    LocalHistory::open_default()
        .refresh()?
        .snapshot
        .context("local usage refresh produced no snapshot")
}
