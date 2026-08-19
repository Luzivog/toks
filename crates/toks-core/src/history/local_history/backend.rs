use anyhow::Result;

use super::super::HistorySnapshot;

pub(super) struct RefreshBatch {
    pub snapshot: HistorySnapshot,
    /// Work still pending after this publishable snapshot. `made_progress`
    /// determines whether the next bounded step is immediate or backed off.
    pub pending_sources: usize,
    /// Whether this step advanced a source checkpoint or compact projection.
    pub made_progress: bool,
}

pub(super) trait HistoryBackend: Send + Sync {
    fn hydrate_archive(&self) -> Result<Option<RefreshBatch>>;
    fn load_last_good(&self) -> Option<HistorySnapshot>;
    fn refresh(&self) -> Result<RefreshBatch>;
    fn store_last_good(&self, snapshot: &HistorySnapshot) -> Result<()>;
}
