use std::collections::{BTreeMap, HashMap, HashSet};

use toks_core::{
    codex_router::thread_lineage::{ThreadLineage, ThreadLineageKind},
    rotation::{ThreadId, ThreadRow},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DisplayThread<'a> {
    pub row: &'a ThreadRow,
    pub depth: usize,
    pub indicator: Option<String>,
}

pub(super) fn group_rows<'a>(
    rows: &'a [ThreadRow],
    lineage: &BTreeMap<ThreadId, ThreadLineage>,
    titles: &BTreeMap<ThreadId, String>,
) -> Vec<DisplayThread<'a>> {
    let visible = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.thread_id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let stored_parents = rows
        .iter()
        .map(|row| parent_index(row, lineage, &visible))
        .collect::<Vec<_>>();
    let cycle_members = cycle_members(&stored_parents);
    let parents = stored_parents
        .iter()
        .enumerate()
        .map(|(index, parent)| {
            if cycle_members.contains(&index) {
                None
            } else {
                *parent
            }
        })
        .collect::<Vec<_>>();
    let mut children = vec![Vec::new(); rows.len()];
    for (child, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            children[*parent].push(child);
        }
    }

    let mut display = Vec::with_capacity(rows.len());
    for root in 0..rows.len() {
        if parents[root].is_none() {
            append_tree(
                root,
                0,
                rows,
                &children,
                root_indicator(&rows[root], lineage, titles, &visible),
                &mut display,
            );
        }
    }
    display
}

fn parent_index(
    row: &ThreadRow,
    lineage: &BTreeMap<ThreadId, ThreadLineage>,
    visible: &BTreeMap<ThreadId, usize>,
) -> Option<usize> {
    let ThreadLineageKind::Subagent {
        parent: Some(parent),
    } = &lineage.get(&row.thread_id)?.kind
    else {
        return None;
    };
    visible.get(parent).copied()
}

fn cycle_members(parents: &[Option<usize>]) -> HashSet<usize> {
    let mut complete = vec![false; parents.len()];
    let mut cycles = HashSet::new();
    for start in 0..parents.len() {
        if complete[start] {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = HashMap::new();
        let mut current = Some(start);
        while let Some(index) = current {
            if let Some(position) = positions.get(&index) {
                cycles.extend(path[*position..].iter().copied());
                break;
            }
            if complete[index] {
                break;
            }
            positions.insert(index, path.len());
            path.push(index);
            current = parents[index];
        }
        for index in path {
            complete[index] = true;
        }
    }
    cycles
}

fn root_indicator(
    row: &ThreadRow,
    lineage: &BTreeMap<ThreadId, ThreadLineage>,
    titles: &BTreeMap<ThreadId, String>,
    visible: &BTreeMap<ThreadId, usize>,
) -> Option<String> {
    let ThreadLineageKind::Subagent { parent } = &lineage.get(&row.thread_id)?.kind else {
        return None;
    };
    match parent {
        None => Some("sub-agent".into()),
        Some(parent) if !visible.contains_key(parent) => Some(format!(
            "sub-agent of {}",
            titles
                .get(parent)
                .cloned()
                .unwrap_or_else(|| short_id(parent))
        )),
        Some(_) => None,
    }
}

fn short_id(thread: &ThreadId) -> String {
    const CHARS: usize = 8;
    let mut characters = thread.as_str().chars();
    let short = characters.by_ref().take(CHARS).collect::<String>();
    if characters.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn append_tree<'a>(
    index: usize,
    depth: usize,
    rows: &'a [ThreadRow],
    children: &[Vec<usize>],
    indicator: Option<String>,
    display: &mut Vec<DisplayThread<'a>>,
) {
    display.push(DisplayThread {
        row: &rows[index],
        depth,
        indicator,
    });
    for child in &children[index] {
        append_tree(*child, depth + 1, rows, children, None, display);
    }
}
