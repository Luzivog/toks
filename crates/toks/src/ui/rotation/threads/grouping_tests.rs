use std::collections::BTreeMap;

use toks_core::{
    accounts::AccountId,
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::{ActiveTaskRow, ThreadId, ThreadRequestSettings, UnixMillis},
};

use super::grouping::group_rows;

fn row(id: impl Into<String>) -> ActiveTaskRow {
    ActiveTaskRow {
        thread_id: ThreadId::new(id),
        account_id: AccountId::new("account"),
        started_at: UnixMillis::new(0),
        request_settings: ThreadRequestSettings::default(),
    }
}

fn subagent(parent: Option<&str>) -> ThreadLineage {
    ThreadLineage {
        kind: ThreadLineageKind::Subagent {
            parent: parent.map(ThreadId::new),
        },
        agent_nickname: None,
    }
}

fn display_ids<'a>(display: &[super::grouping::DisplayThread<'a>]) -> Vec<&'a str> {
    display
        .iter()
        .map(|thread| thread.row.thread_id.as_str())
        .collect()
}

#[test]
fn children_nest_below_their_parent_in_existing_order() {
    let rows = vec![row("child-b"), row("parent"), row("other"), row("child-a")];
    let lineage = BTreeMap::from([
        (ThreadId::new("child-b"), subagent(Some("parent"))),
        (ThreadId::new("child-a"), subagent(Some("parent"))),
    ]);

    let display = group_rows(&rows, &lineage, &BTreeMap::new());

    assert_eq!(
        display_ids(&display),
        ["parent", "child-b", "child-a", "other"]
    );
    assert_eq!(
        display
            .iter()
            .map(|thread| thread.depth)
            .collect::<Vec<_>>(),
        [0, 1, 1, 0]
    );
}

#[test]
fn nesting_keeps_the_full_recursive_depth() {
    let rows = vec![row("grandchild"), row("child"), row("parent")];
    let lineage = BTreeMap::from([
        (ThreadId::new("grandchild"), subagent(Some("child"))),
        (ThreadId::new("child"), subagent(Some("parent"))),
    ]);

    let display = group_rows(&rows, &lineage, &BTreeMap::new());

    assert_eq!(display_ids(&display), ["parent", "child", "grandchild"]);
    assert_eq!(
        display
            .iter()
            .map(|thread| thread.depth)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn orphan_subagents_stay_top_level_with_specific_indicators() {
    let rows = vec![row("named"), row("short-id"), row("unknown"), row("plain")];
    let lineage = BTreeMap::from([
        (ThreadId::new("named"), subagent(Some("parent-named"))),
        (
            ThreadId::new("short-id"),
            subagent(Some("01a03a64-1234-5678")),
        ),
        (ThreadId::new("unknown"), subagent(None)),
    ]);
    let titles = BTreeMap::from([(ThreadId::new("parent-named"), "Plan the migration".into())]);

    let display = group_rows(&rows, &lineage, &titles);

    assert_eq!(
        display_ids(&display),
        ["named", "short-id", "unknown", "plain"]
    );
    assert_eq!(
        display[0].indicator.as_deref(),
        Some("sub-agent of Plan the migration")
    );
    assert_eq!(
        display[1].indicator.as_deref(),
        Some("sub-agent of 01a03a64…")
    );
    assert_eq!(display[2].indicator.as_deref(), Some("sub-agent"));
    assert_eq!(display[3].indicator, None);
}

#[test]
fn cycles_render_once_in_flat_original_order() {
    let rows = vec![row("a"), row("b"), row("plain")];
    let lineage = BTreeMap::from([
        (ThreadId::new("a"), subagent(Some("b"))),
        (ThreadId::new("b"), subagent(Some("a"))),
    ]);

    let display = group_rows(&rows, &lineage, &BTreeMap::new());

    assert_eq!(display_ids(&display), ["a", "b", "plain"]);
    assert!(display.iter().all(|thread| thread.depth == 0));
}

#[test]
fn grouping_keeps_every_active_task() {
    let mut rows = vec![row("child")];
    rows.extend((1..=99).map(|index| row(format!("root-{index:02}"))));
    rows.push(row("parent"));
    let lineage = BTreeMap::from([(ThreadId::new("child"), subagent(Some("parent")))]);

    let display = group_rows(&rows, &lineage, &BTreeMap::new());

    assert_eq!(display.len(), 101);
    assert_eq!(display[99].row.thread_id.as_str(), "parent");
    assert!(display
        .iter()
        .any(|thread| thread.row.thread_id.as_str() == "child"));
}
