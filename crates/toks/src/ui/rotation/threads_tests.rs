use std::collections::BTreeMap;

use toks_core::{
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::{ThreadId, ThreadStatus},
};

use super::threads::{
    header_count, selector_label, service_tier_value, status_label, thread_title, visible_rows,
    visible_status, SelectorSource, VisibleThreadStatus,
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
fn active_thread_list_only_includes_streaming_and_waiting_statuses() {
    assert_eq!(
        visible_status(ThreadStatus::Streaming { stream_count: 2 }),
        Some(VisibleThreadStatus::Streaming)
    );
    assert_eq!(
        visible_status(ThreadStatus::AwaitingFollowUp),
        Some(VisibleThreadStatus::Waiting)
    );
    assert_eq!(visible_status(ThreadStatus::ReservationPending), None);
    assert_eq!(visible_status(ThreadStatus::AttachedIdle), None);
}

#[test]
fn selector_labels_prefer_overrides_then_observed_values_then_a_placeholder() {
    let overridden = selector_label(Some("gpt-5.6"), Some("gpt-5.5"));
    assert_eq!(overridden.text, "gpt-5.6");
    assert_eq!(overridden.source, SelectorSource::Override);

    let observed = selector_label(None, Some("gpt-5.5"));
    assert_eq!(observed.text, "gpt-5.5");
    assert_eq!(observed.source, SelectorSource::Observed);

    let placeholder = selector_label(None, None);
    assert_eq!(placeholder.text, "—");
    assert_eq!(placeholder.source, SelectorSource::Placeholder);
}

#[test]
fn omitted_service_tier_is_default_and_explicit_priority_is_preserved() {
    assert_eq!(service_tier_value(None), "default");
    assert_eq!(service_tier_value(Some("default")), "default");
    assert_eq!(service_tier_value(Some("priority")), "priority");
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
fn thread_header_annotation_reports_streaming_and_waiting_counts() {
    assert_eq!(
        header_count([
            ThreadStatus::Streaming { stream_count: 2 },
            ThreadStatus::ReservationPending,
        ]),
        "1 streaming"
    );
    assert_eq!(
        header_count([
            ThreadStatus::Streaming { stream_count: 1 },
            ThreadStatus::ReservationPending,
            ThreadStatus::AwaitingFollowUp,
            ThreadStatus::AttachedIdle,
        ]),
        "1 streaming · 1 waiting"
    );
}

#[test]
fn thread_header_annotation_reports_the_display_cap() {
    let mut statuses = vec![ThreadStatus::Streaming { stream_count: 1 }; 100];
    assert_eq!(header_count(statuses.iter().copied()), "100 streaming");

    statuses.push(ThreadStatus::AwaitingFollowUp);
    assert_eq!(
        header_count(statuses),
        "100 streaming · 1 waiting · showing 100"
    );

    assert_eq!(
        header_count(
            std::iter::repeat_n(ThreadStatus::Streaming { stream_count: 1 }, 100)
                .chain([ThreadStatus::AttachedIdle]),
        ),
        "100 streaming"
    );
}
