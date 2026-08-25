use std::collections::BTreeMap;

use toks_core::{
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::{ThreadId, ThreadStatus},
};

const VISIBLE_THREAD_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::rotation) enum SelectorSource {
    Override,
    Observed,
    Placeholder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::rotation) struct SelectorLabel {
    pub(in crate::ui::rotation) text: String,
    pub(in crate::ui::rotation) source: SelectorSource,
}

pub(in crate::ui::rotation) fn header_count(
    statuses: impl IntoIterator<Item = ThreadStatus>,
) -> String {
    let mut total = 0;
    let mut streaming = 0;
    let mut idle = 0;
    for status in statuses {
        total += 1;
        match status {
            ThreadStatus::Streaming { .. } | ThreadStatus::ReservationPending => streaming += 1,
            ThreadStatus::AwaitingFollowUp | ThreadStatus::AttachedIdle => idle += 1,
        }
    }

    let mut label = if idle == 0 {
        format!("{streaming} streaming")
    } else {
        format!("{streaming} streaming · {idle} idle")
    };
    if total > VISIBLE_THREAD_LIMIT {
        label.push_str(&format!(" · showing {VISIBLE_THREAD_LIMIT}"));
    }
    label
}

pub(in crate::ui::rotation) fn visible_rows<T>(rows: &[T]) -> impl Iterator<Item = &T> {
    rows.iter().take(VISIBLE_THREAD_LIMIT)
}

pub(in crate::ui::rotation) fn thread_title(
    titles: &BTreeMap<ThreadId, String>,
    lineage: Option<&ThreadLineage>,
    thread: &ThreadId,
) -> String {
    titles
        .get(thread)
        .cloned()
        .or_else(|| {
            let lineage = lineage?;
            match &lineage.kind {
                ThreadLineageKind::Subagent { .. } => lineage.agent_nickname.clone(),
                ThreadLineageKind::TopLevel => None,
            }
        })
        .unwrap_or_else(|| thread.as_str().to_owned())
}

pub(in crate::ui::rotation) const fn status_label(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::Streaming { .. } => "Streaming",
        ThreadStatus::ReservationPending => "Starting",
        ThreadStatus::AwaitingFollowUp => "Waiting",
        ThreadStatus::AttachedIdle => "Idle",
    }
}

pub(in crate::ui::rotation) fn selector_label(
    thread_override: Option<&str>,
    observed: Option<&str>,
) -> SelectorLabel {
    if let Some(value) = thread_override {
        SelectorLabel {
            text: value.to_owned(),
            source: SelectorSource::Override,
        }
    } else if let Some(value) = observed {
        SelectorLabel {
            text: value.to_owned(),
            source: SelectorSource::Observed,
        }
    } else {
        SelectorLabel {
            text: "—".into(),
            source: SelectorSource::Placeholder,
        }
    }
}
