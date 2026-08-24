use super::{HistorySnapshot, LocalHistory};

/// Compatibility startup shape retained while the desktop app moves to
/// [`LocalHistory`](super::LocalHistory).
pub struct HistoryHydration {
    pub snapshot: Option<HistorySnapshot>,
    pub warning: Option<String>,
}

pub fn hydrate() -> HistoryHydration {
    let view = LocalHistory::open_default().hydrate();
    HistoryHydration {
        snapshot: view.snapshot,
        warning: view.warning,
    }
}
