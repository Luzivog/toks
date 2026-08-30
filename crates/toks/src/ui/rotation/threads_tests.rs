use std::collections::BTreeMap;

use toks_core::{
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::ThreadId,
};

use super::threads::{selector_label, service_tier_value, thread_title, SelectorSource};

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
