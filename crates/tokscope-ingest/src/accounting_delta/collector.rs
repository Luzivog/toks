use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{get_home_dir_string, pricing::PricingService, scanner, ClientId};

use super::fingerprint::{metadata, samples_match};
use super::identity::source_key;
use super::store::CheckpointStore;
use super::types::{
    AccountingBacklog, AccountingDelta, AccountingDeltaOptions, CollectContext, SourceCandidate,
    SourceKind,
};

const SOURCES_PER_COLLECT: usize = 32;

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

    /// Collect a bounded, newest-first batch without advancing durable offsets.
    ///
    /// The caller must durably apply every returned observation before calling
    /// [`Self::commit`]. Reversing that order can lose usage after a crash.
    pub fn collect(
        &mut self,
        options: AccountingDeltaOptions,
        pricing: Option<&PricingService>,
    ) -> Result<AccountingDelta, String> {
        let home = PathBuf::from(get_home_dir_string(&options.home_dir)?);
        let candidates = self.discover(&home, &options)?;
        let discovered_sources = candidates.len();
        let mut changed: Vec<_> = candidates
            .into_iter()
            .filter(|source| self.is_changed(source))
            .collect();
        let changed_sources = changed.len();
        if let Some(cursor) = self.store.rotation_cursor() {
            if let Some(index) = changed
                .iter()
                .position(|source| source.key.as_str() == cursor)
            {
                let next = (index + 1) % changed.len().max(1);
                changed.rotate_left(next);
            }
        }
        let attempted = changed_sources.min(SOURCES_PER_COLLECT);
        let scan_progress = changed_sources > attempted && attempted > 0;
        if let Some(last) = changed.get(attempted.saturating_sub(1)) {
            let key = last.key.clone();
            self.store.set_rotation_cursor(&key)?;
        }
        let context = CollectContext {
            pricing,
            home_dir: &home,
        };
        let selected: Vec<_> = changed.into_iter().take(SOURCES_PER_COLLECT).collect();
        let mut codex_seeds = crate::message_cache::load_codex_accounting_seeds(
            selected
                .iter()
                .filter(|source| {
                    source.kind == SourceKind::Codex && self.store.get(&source.key).is_none()
                })
                .map(|source| source.path.as_path()),
        );
        let mut sources = Vec::with_capacity(attempted);
        let mut still_pending = changed_sources.saturating_sub(attempted);
        for source in selected {
            let previous = self.store.get(&source.key);
            let processed = match source.kind {
                SourceKind::Codex => super::codex::process(
                    &source,
                    previous,
                    codex_seeds.remove(&source.path),
                    &context,
                )?,
                SourceKind::Claude => super::claude::process(&source, previous, &context)?,
            };
            still_pending += usize::from(processed.remains_pending);
            if let Some(delta) = processed.delta {
                sources.push(delta);
            }
        }
        Ok(AccountingDelta {
            sources,
            backlog: AccountingBacklog {
                discovered_sources,
                changed_sources,
                pending_sources: still_pending,
                scan_progress,
            },
        })
    }

    /// Acknowledge a batch only after its observations are durably archived.
    pub fn commit(&mut self, delta: &AccountingDelta) -> Result<(), String> {
        if delta.sources.is_empty() {
            return Ok(());
        }
        self.store.commit(
            delta
                .sources
                .iter()
                .map(|source| (&source.source_key, &source.proposed)),
        )
    }

    fn discover(
        &self,
        home: &Path,
        options: &AccountingDeltaOptions,
    ) -> Result<Vec<SourceCandidate>, String> {
        let clients = vec!["codex".to_string(), "claude".to_string()];
        let scan = scanner::scan_all_clients_with_scanner_settings(
            home.to_string_lossy().as_ref(),
            &clients,
            options.use_env_roots,
            &options.scanner_settings,
        );
        let mut by_key = HashMap::new();
        self.add_candidates(&mut by_key, SourceKind::Codex, scan.get(ClientId::Codex))?;
        self.add_candidates(&mut by_key, SourceKind::Claude, scan.get(ClientId::Claude))?;
        let mut candidates: Vec<_> = by_key.into_values().collect();
        candidates.sort_by(|left, right| {
            right
                .modified_ns
                .cmp(&left.modified_ns)
                .then_with(|| right.size.cmp(&left.size))
                .then_with(|| left.key.as_str().cmp(right.key.as_str()))
        });
        Ok(candidates)
    }

    fn add_candidates(
        &self,
        candidates: &mut HashMap<super::types::SourceKey, SourceCandidate>,
        kind: SourceKind,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        for path in paths {
            let file = metadata(path)?;
            let key = source_key(self.store.key(), kind, path);
            let candidate = SourceCandidate {
                kind,
                path: path.clone(),
                key: key.clone(),
                size: file.size,
                modified_ns: file.modified_ns,
            };
            let replace = candidates.get(&key).is_none_or(|existing| {
                (candidate.modified_ns, candidate.size) > (existing.modified_ns, existing.size)
            });
            if replace {
                candidates.insert(key, candidate);
            }
        }
        Ok(())
    }

    fn is_changed(&self, source: &SourceCandidate) -> bool {
        let Some(checkpoint) = self.store.get(&source.key) else {
            return true;
        };
        let metadata_changed = checkpoint.committed_offset != source.size
            || checkpoint.source_size != source.size
            || checkpoint.modified_ns != source.modified_ns;
        checkpoint.kind != source.kind
            || checkpoint.parser_version != parser_version(source.kind)
            || metadata_changed
            || !samples_match(&source.path, &checkpoint.prefix_samples)
    }
}

fn parser_version(kind: SourceKind) -> u32 {
    match kind {
        SourceKind::Codex => super::codex::parser_version(),
        SourceKind::Claude => super::claude::parser_version(),
    }
}
