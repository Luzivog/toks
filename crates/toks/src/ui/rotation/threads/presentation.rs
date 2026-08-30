use std::collections::BTreeMap;

use toks_core::{
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::ThreadId,
};

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

/// Codex omits `service_tier` for standard requests.
pub(in crate::ui::rotation) fn service_tier_value(observed: Option<&str>) -> &str {
    observed.unwrap_or("default")
}
