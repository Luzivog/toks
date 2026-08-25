use std::collections::BTreeMap;

use toks_core::{
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::{ThreadId, ThreadStatus},
};

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
    assert_eq!(thread_title(&titles, None, &thread), "thread-42");

    titles.insert(thread.clone(), "Repair router handoff".into());
    assert_eq!(
        thread_title(&titles, None, &thread),
        "Repair router handoff"
    );
}

#[test]
fn untitled_subagents_use_their_agent_nickname() {
    let thread = ThreadId::new("thread-42");
    let subagent = ThreadLineage {
        kind: ThreadLineageKind::Subagent { parent: None },
        agent_nickname: Some("Dirac".into()),
    };
    assert_eq!(
        thread_title(&BTreeMap::new(), Some(&subagent), &thread),
        "Dirac"
    );

    let user = ThreadLineage {
        kind: ThreadLineageKind::TopLevel,
        agent_nickname: Some("Not a title".into()),
    };
    assert_eq!(
        thread_title(&BTreeMap::new(), Some(&user), &thread),
        "thread-42"
    );
}

#[test]
fn active_thread_rows_are_display_capped_at_one_hundred() {
    let rows = (0..101).collect::<Vec<_>>();
    assert_eq!(visible_rows(&rows).count(), 100);
    assert_eq!(visible_rows(&rows[..99]).count(), 99);
}

#[test]
fn empty_thread_header_annotation_reports_zero_streaming() {
    assert_eq!(header_count([]), "0 streaming");
}

#[test]
fn thread_header_annotation_reports_streaming_and_idle_counts() {
    assert_eq!(
        header_count([
            ThreadStatus::Streaming { stream_count: 2 },
            ThreadStatus::ReservationPending,
        ]),
        "2 streaming"
    );
    assert_eq!(
        header_count([
            ThreadStatus::Streaming { stream_count: 1 },
            ThreadStatus::ReservationPending,
            ThreadStatus::AwaitingFollowUp,
            ThreadStatus::AttachedIdle,
        ]),
        "2 streaming · 2 idle"
    );
}

#[test]
fn thread_header_annotation_reports_the_display_cap() {
    let mut statuses = vec![ThreadStatus::ReservationPending; 100];
    assert_eq!(header_count(statuses.iter().copied()), "100 streaming");

    statuses.push(ThreadStatus::AttachedIdle);
    assert_eq!(
        header_count(statuses),
        "100 streaming · 1 idle · showing 100"
    );
}
