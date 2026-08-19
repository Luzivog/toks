mod backend;
mod default;
mod pipeline;
mod projected;

use std::sync::Arc;

use anyhow::{bail, Result};

use super::HistorySnapshot;
use backend::{HistoryBackend, RefreshBatch};
use default::DefaultBackend;

/// Scheduling hint for another bounded catch-up step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatchUpRetry {
    Immediate,
    ShortBackoff,
}

/// Whether local usage is current, still indexing older sources, or showing
/// the last durable result while another writer or failed refresh owns work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryStatus {
    Ready,
    CatchingUp {
        pending_sources: usize,
        captured_through_ms: Option<i64>,
        retry: CatchUpRetry,
    },
    BusyUsingLastGood {
        captured_through_ms: Option<i64>,
        reason: String,
    },
}

/// A publishable local history view. A catching-up view is intentionally
/// usable: recent committed usage must not wait for historical backfill.
pub struct HistoryView {
    pub snapshot: Option<HistorySnapshot>,
    pub status: HistoryStatus,
    pub warning: Option<String>,
}

/// Deep local-history module. Callers do not coordinate scanners, archives,
/// pricing, migrations, caches, or backpressure themselves.
#[derive(Clone)]
pub struct LocalHistory {
    backend: Arc<dyn HistoryBackend>,
}

impl LocalHistory {
    pub fn open_default() -> Self {
        Self {
            backend: Arc::new(DefaultBackend::open()),
        }
    }

    pub fn hydrate(&self) -> HistoryView {
        match self.backend.hydrate_archive() {
            Ok(Some(mut batch)) => {
                carry_last_good_freshness(
                    &mut batch.snapshot,
                    self.backend.load_last_good().as_ref(),
                );
                view(batch, None)
            }
            Ok(None) => ready(self.backend.load_last_good()),
            Err(error) => last_good(self.backend.load_last_good(), error.to_string()),
        }
    }

    /// Runs one bounded refresh step. The `CatchingUp` retry hint distinguishes
    /// real forward progress from a live source awaiting a complete record.
    pub fn refresh(&self) -> Result<HistoryView> {
        match self.backend.refresh() {
            Ok(batch) => self.publish(batch),
            Err(error) => {
                let reason = error.to_string();
                let collector_busy =
                    reason == tokscope_ingest::accounting_delta::COLLECTOR_BUSY_ERROR;
                let view = last_good(self.backend.load_last_good(), reason);
                if view.snapshot.is_none() && !collector_busy {
                    bail!(view
                        .warning
                        .unwrap_or_else(|| "usage refresh failed".into()));
                }
                Ok(view)
            }
        }
    }

    fn publish(&self, batch: RefreshBatch) -> Result<HistoryView> {
        super::validation::validate(&batch.snapshot)?;
        // Storage failure must not hide a fresh valid snapshot. Atomic storage
        // leaves the previous last-good snapshot intact.
        let warning = self
            .backend
            .store_last_good(&batch.snapshot)
            .err()
            .map(|e| format!("Fresh usage is visible but could not be saved locally: {e}"));
        Ok(view(batch, warning))
    }

    #[cfg(test)]
    fn with_backend(backend: impl HistoryBackend + 'static) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }
}

fn ready(snapshot: Option<HistorySnapshot>) -> HistoryView {
    HistoryView {
        snapshot,
        status: HistoryStatus::Ready,
        warning: None,
    }
}

fn view(batch: RefreshBatch, warning: Option<String>) -> HistoryView {
    let captured_through_ms = batch.snapshot.captured_through_ms;
    let status = if batch.pending_sources == 0 {
        HistoryStatus::Ready
    } else {
        HistoryStatus::CatchingUp {
            pending_sources: batch.pending_sources,
            captured_through_ms,
            retry: if batch.made_progress {
                CatchUpRetry::Immediate
            } else {
                CatchUpRetry::ShortBackoff
            },
        }
    };
    HistoryView {
        snapshot: Some(batch.snapshot),
        status,
        warning,
    }
}

fn last_good(snapshot: Option<HistorySnapshot>, reason: String) -> HistoryView {
    let captured_through_ms = snapshot.as_ref().and_then(|s| s.captured_through_ms);
    HistoryView {
        snapshot,
        status: HistoryStatus::BusyUsingLastGood {
            captured_through_ms,
            reason: reason.clone(),
        },
        warning: Some(format!("Durable usage refresh unavailable: {reason}")),
    }
}

fn carry_last_good_freshness(archive: &mut HistorySnapshot, fallback: Option<&HistorySnapshot>) {
    let Some(fallback) = fallback else {
        return;
    };
    let same_facts = archive.strong_events == fallback.strong_events
        && archive.weak_events == fallback.weak_events
        && archive.history_conflicts == fallback.history_conflicts;
    if same_facts && fallback.captured_through_ms > archive.captured_through_ms {
        archive.captured_through_ms = fallback.captured_through_ms;
    }
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;
