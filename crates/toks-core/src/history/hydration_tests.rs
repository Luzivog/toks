use super::hydration::HistoryHydration;

#[test]
fn compatibility_shape_keeps_snapshot_and_warning() {
    let hydrated = HistoryHydration {
        snapshot: None,
        warning: Some("saved".into()),
    };
    assert!(hydrated.snapshot.is_none());
    assert_eq!(hydrated.warning.as_deref(), Some("saved"));
}
