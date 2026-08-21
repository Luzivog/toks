use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{scanner, ClientId};

use super::AccountingDeltaCollector;
use crate::accounting_delta::fingerprint::{metadata, samples_match};
use crate::accounting_delta::identity::source_key;
use crate::accounting_delta::types::{AccountingDeltaOptions, SourceCandidate, SourceKind};

impl AccountingDeltaCollector {
    pub(super) fn discover(
        &self,
        home: &Path,
        options: &AccountingDeltaOptions,
    ) -> Result<Vec<SourceCandidate>, String> {
        let clients = vec![
            "codex".to_string(),
            "claude".to_string(),
            "opencode".to_string(),
        ];
        let scan = scanner::scan_all_clients_with_scanner_settings(
            home.to_string_lossy().as_ref(),
            &clients,
            options.use_env_roots,
            &options.scanner_settings,
        );
        let mut by_key = HashMap::new();
        self.add_candidates(&mut by_key, SourceKind::Codex, scan.get(ClientId::Codex))?;
        self.add_candidates(&mut by_key, SourceKind::Claude, scan.get(ClientId::Claude))?;
        self.add_candidates(&mut by_key, SourceKind::OpenCode, &scan.opencode_dbs)?;
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
        candidates: &mut HashMap<crate::accounting_delta::SourceKey, SourceCandidate>,
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

    pub(super) fn is_changed(&self, source: &SourceCandidate) -> Result<bool, String> {
        let Some(checkpoint) = self.store.summary(&source.key)? else {
            return Ok(true);
        };
        let metadata_changed = checkpoint.committed_offset != source.size
            || checkpoint.source_size != source.size
            || checkpoint.modified_ns != source.modified_ns;
        Ok(checkpoint.kind != source.kind
            || checkpoint_version_changed(source.kind, checkpoint.parser_version)
            || metadata_changed
            || !samples_match(&source.path, &checkpoint.prefix_samples))
    }
}

fn parser_version(kind: SourceKind) -> u32 {
    match kind {
        SourceKind::Codex => crate::accounting_delta::codex::parser_version(),
        SourceKind::Claude => crate::accounting_delta::claude::parser_version(),
        SourceKind::OpenCode => crate::accounting_delta::opencode::parser_version(),
    }
}

fn checkpoint_version_changed(kind: SourceKind, version: u32) -> bool {
    match kind {
        SourceKind::Codex => {
            !crate::accounting_delta::codex::checkpoint_version_is_current(version)
        }
        SourceKind::Claude | SourceKind::OpenCode => version != parser_version(kind),
    }
}
