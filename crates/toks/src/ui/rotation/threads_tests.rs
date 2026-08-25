use std::collections::BTreeMap;

use toks_core::rotation::{ThreadId, ThreadStatus};

use super::threads::{
    header_count, selector_label, status_label, thread_title, visible_rows, SelectorSource,
};

#[test]
fn active_thread_statuses_use_terse_labels() {
    assert_eq!(
        status_label(ThreadStatus::Streaming { stream_count: 2 }),
        "Streaming"
    );
    assert_eq!(status_label(ThreadStatus::ReservationPending), "Starting");
    assert_eq!(status_label(ThreadStatus::AwaitingFollowUp), "Waiting");
    assert_eq!(status_label(ThreadStatus::AttachedIdle), "Idle");
}

#[test]
fn selector_labels_are_values_without_column_prefixes() {
    let overridden = selector_label(Some("gpt-5.6"), Some("gpt-5.5"));
    assert_eq!(overridden.text, "gpt-5.6");
    assert_eq!(overridden.source, SelectorSource::Override);

    let observed = selector_label(None, Some("gpt-5.5"));
    assert_eq!(observed.text, "gpt-5.5");
    assert_eq!(observed.source, SelectorSource::Observed);

    let automatic = selector_label(None, None);
    assert_eq!(automatic.text, "Auto");
    assert_eq!(automatic.source, SelectorSource::Auto);
}

#[test]
fn thread_title_falls_back_to_the_thread_id() {
    let thread = ThreadId::new("thread-42");
    let mut titles = BTreeMap::new();
    assert_eq!(thread_title(&titles, &thread), "thread-42");

    titles.insert(thread.clone(), "Repair router handoff".into());
    assert_eq!(thread_title(&titles, &thread), "Repair router handoff");
}

#[test]
fn active_thread_rows_are_display_capped_at_one_hundred() {
    let rows = (0..101).collect::<Vec<_>>();
    assert_eq!(visible_rows(&rows).count(), 100);
    assert_eq!(visible_rows(&rows[..99]).count(), 99);
}

#[test]
fn active_thread_header_reports_visible_and_total_counts() {
    assert_eq!(header_count(0), "0 active");
    assert_eq!(header_count(3), "3 active");
    assert_eq!(header_count(100), "100 active");
    assert_eq!(header_count(101), "100 of 101");
}
