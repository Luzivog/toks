use tokscope_core::HistorySnapshot;

use super::should_publish;

fn snapshot(generated_at_ms: i64) -> HistorySnapshot {
    HistorySnapshot {
        generated_at_ms,
        ..Default::default()
    }
}

#[test]
fn saved_fallback_cannot_replace_newer_in_memory_usage() {
    let current = snapshot(200);
    assert!(!should_publish(Some(&current), &snapshot(100)));
    assert!(should_publish(Some(&current), &snapshot(200)));
    assert!(should_publish(Some(&current), &snapshot(300)));
}

#[test]
fn first_publish_is_always_accepted() {
    assert!(should_publish(None, &snapshot(1)));
}
