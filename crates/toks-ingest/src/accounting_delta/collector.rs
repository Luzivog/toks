use std::path::PathBuf;

use crate::{get_home_dir_string, pricing::PricingService};

use super::store::CheckpointStore;
use super::types::{
    AccountingAdvance, AccountingAdvanceError, AccountingDeltaOptions, AccountingSource,
    CollectContext, SourceDelta, SourceKind,
};

mod discovery;

const SOURCES_PER_ADVANCE: usize = 32;

pub struct AccountingDeltaCollector {
    store: CheckpointStore,
}

impl AccountingDeltaCollector {
    pub fn open_default() -> Result<Self, String> {
        Self::open_at(crate::paths::get_cache_dir().join("accounting-delta"))
    }

    pub fn open_at(directory: impl Into<PathBuf>) -> Result<Self, String> {
        Ok(Self {
            store: CheckpointStore::open(directory.into())?,
        })
    }

    /// Processes a bounded work quantum, archiving and acknowledging one source
    /// at a time so observations from earlier sources can be released promptly.
    pub fn advance<E>(
        &mut self,
        options: AccountingDeltaOptions,
        pricing: Option<&PricingService>,
        mut archive: impl FnMut(AccountingSource<'_>) -> Result<(), E>,
    ) -> Result<AccountingAdvance, AccountingAdvanceError<E>> {
        self.advance_with(options, pricing, |source| {
            archive(AccountingSource {
                source_key: &source.source_key,
                revision: &source.revision,
                observations: &source.observations,
                backfill_complete: source.backfill_complete,
            })
        })
    }

    fn advance_with<E>(
        &mut self,
        options: AccountingDeltaOptions,
        pricing: Option<&PricingService>,
        mut archive: impl FnMut(&SourceDelta) -> Result<(), E>,
    ) -> Result<AccountingAdvance, AccountingAdvanceError<E>> {
        let home = PathBuf::from(
            get_home_dir_string(&options.home_dir).map_err(AccountingAdvanceError::Ingest)?,
        );
        let candidates = self
            .discover(&home, &options)
            .map_err(AccountingAdvanceError::Ingest)?;
        let discovered_sources = candidates.len();
        let mut changed = Vec::new();
        for source in candidates {
            if self
                .is_changed(&source)
                .map_err(AccountingAdvanceError::Ingest)?
            {
                changed.push(source);
            }
        }
        let changed_sources = changed.len();
        if let Some(cursor) = self
            .store
            .rotation_cursor()
            .map_err(AccountingAdvanceError::Ingest)?
        {
            if let Some(index) = changed
                .iter()
                .position(|source| source.key.as_str() == cursor)
            {
                let next = (index + 1) % changed.len().max(1);
                changed.rotate_left(next);
            }
        }
        let attempted = changed_sources.min(SOURCES_PER_ADVANCE);
        let scan_progress = changed_sources > attempted && attempted > 0;
        let selected = changed.into_iter().take(SOURCES_PER_ADVANCE);
        let context = CollectContext {
            pricing,
            home_dir: &home,
        };
        let mut archived_sources = 0;
        let mut still_pending = changed_sources.saturating_sub(attempted);
        for source in selected {
            let previous = self
                .store
                .get(&source.key)
                .map_err(AccountingAdvanceError::Ingest)?;
            let processed = match source.kind {
                SourceKind::Codex => super::codex::process(
                    &source,
                    previous.as_ref(),
                    previous
                        .is_none()
                        .then(|| crate::message_cache::load_codex_accounting_seed(&source.path))
                        .flatten(),
                    &context,
                ),
                SourceKind::Claude => super::claude::process(&source, previous.as_ref(), &context),
                SourceKind::OpenCode => {
                    super::opencode::process(&source, previous.as_ref(), &context)
                }
            }
            .map_err(AccountingAdvanceError::Ingest)?;
            still_pending += usize::from(processed.remains_pending);
            if let Some(delta) = processed.delta.as_ref() {
                archive(delta).map_err(AccountingAdvanceError::Archive)?;
                self.store
                    .acknowledge(&source.key, Some(&delta.proposed))
                    .map_err(AccountingAdvanceError::CheckpointAfterArchive)?;
                archived_sources += 1;
            } else {
                self.store
                    .acknowledge(&source.key, None)
                    .map_err(AccountingAdvanceError::Ingest)?;
            }
        }
        Ok(AccountingAdvance {
            archived_sources,
            backlog: super::types::AccountingBacklog {
                discovered_sources,
                changed_sources,
                pending_sources: still_pending,
                scan_progress,
            },
        })
    }

    #[cfg(test)]
    pub(crate) fn advance_for_test(
        &mut self,
        options: AccountingDeltaOptions,
        pricing: Option<&PricingService>,
    ) -> Result<super::types::AccountingDelta, String> {
        let mut sources = Vec::new();
        let result = self.advance_with(options, pricing, |source| {
            sources.push(source.clone());
            Ok::<(), std::convert::Infallible>(())
        });
        let advance = match result {
            Ok(advance) => advance,
            Err(AccountingAdvanceError::Ingest(error))
            | Err(AccountingAdvanceError::CheckpointAfterArchive(error)) => return Err(error),
            Err(AccountingAdvanceError::Archive(never)) => match never {},
        };
        Ok(super::types::AccountingDelta {
            sources,
            backlog: advance.backlog,
        })
    }
}
