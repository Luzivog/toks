use std::collections::{BTreeMap, BTreeSet};

use toks_core::{
    codex_router::{
        thread_lineage::{ThreadLineage, ThreadLineageKind, ThreadLineageStore},
        thread_titles::ThreadTitleStore,
    },
    rotation::{RotationRuntime, ThreadId},
};

#[derive(Clone)]
pub(super) struct ThreadMetadataStores {
    titles: ThreadTitleStore,
    lineage: ThreadLineageStore,
}

pub(super) struct ThreadMetadata {
    pub titles: BTreeMap<ThreadId, String>,
    pub lineage: BTreeMap<ThreadId, ThreadLineage>,
}

impl ThreadMetadataStores {
    pub(super) fn discover() -> Self {
        Self {
            titles: ThreadTitleStore::discover(),
            lineage: ThreadLineageStore::discover(),
        }
    }

    pub(super) fn load(&self, runtime: &RotationRuntime) -> ThreadMetadata {
        let visible_ids = runtime
            .live_thread_rows()
            .into_iter()
            .map(|row| row.thread_id)
            .collect::<Vec<_>>();
        let lineage = self.lineage.lineages(&visible_ids);
        let mut title_ids = visible_ids.into_iter().collect::<BTreeSet<_>>();
        title_ids.extend(lineage.values().filter_map(|lineage| match &lineage.kind {
            ThreadLineageKind::Subagent {
                parent: Some(parent),
            } => Some(parent.clone()),
            ThreadLineageKind::TopLevel | ThreadLineageKind::Subagent { parent: None } => None,
        }));
        let title_ids = title_ids.into_iter().collect::<Vec<_>>();
        ThreadMetadata {
            titles: self.titles.titles(&title_ids),
            lineage,
        }
    }
}
