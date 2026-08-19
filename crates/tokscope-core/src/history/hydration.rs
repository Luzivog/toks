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

#[cfg(test)]
mod tests {
    use super::HistoryHydration;

    #[test]
    fn compatibility_shape_keeps_snapshot_and_warning() {
        let hydrated = HistoryHydration {
            snapshot: None,
            warning: Some("saved".into()),
        };
        assert!(hydrated.snapshot.is_none());
        assert_eq!(hydrated.warning.as_deref(), Some("saved"));
    }
}
