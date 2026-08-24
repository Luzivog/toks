use std::sync::Mutex;

use anyhow::{anyhow, Result};
use toks_ingest::accounting_delta::AccountingDeltaCollector;

use super::backend::{HistoryBackend, RefreshBatch};
use crate::history::HistorySnapshot;

pub(super) struct DefaultBackend {
    collector: Mutex<CollectorSlot>,
}

enum CollectorSlot {
    Ready(AccountingDeltaCollector),
    Unavailable(String),
}

impl DefaultBackend {
    pub(super) fn open() -> Self {
        let collector = match AccountingDeltaCollector::open_default() {
            Ok(collector) => CollectorSlot::Ready(collector),
            Err(error) => CollectorSlot::Unavailable(error),
        };
        Self {
            collector: Mutex::new(collector),
        }
    }
}

impl HistoryBackend for DefaultBackend {
    fn hydrate_archive(&self) -> Result<Option<RefreshBatch>> {
        super::pipeline::hydrate_archive()
    }

    fn load_last_good(&self) -> Option<HistorySnapshot> {
        crate::history::cache::load()
    }

    fn refresh(&self) -> Result<RefreshBatch> {
        let mut slot = self
            .collector
            .lock()
            .map_err(|_| anyhow!("incremental usage collector lock was poisoned"))?;
        if matches!(&*slot, CollectorSlot::Unavailable(_)) {
            *slot = match AccountingDeltaCollector::open_default() {
                Ok(collector) => CollectorSlot::Ready(collector),
                Err(error) => CollectorSlot::Unavailable(error),
            };
        }
        match &mut *slot {
            CollectorSlot::Ready(collector) => super::pipeline::refresh(collector),
            CollectorSlot::Unavailable(error) => Err(anyhow!(error.clone())),
        }
    }

    fn store_last_good(&self, snapshot: &HistorySnapshot) -> Result<()> {
        crate::history::cache::store(snapshot)
    }
}
