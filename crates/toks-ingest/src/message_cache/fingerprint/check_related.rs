use super::{FingerprintStatus, SourceFingerprint};
use crate::message_cache::shard::{append_path_suffix, ContentHashMode};
use std::path::{Path, PathBuf};

impl SourceFingerprint {
    pub(crate) fn check_sqlite_path(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        let related_paths = ["-wal"]
            .into_iter()
            .map(|suffix| (suffix.to_string(), append_path_suffix(path, suffix)));
        // SQLite databases can be tens of GB; skip the whole-file content hash
        // (size + mtime + samples detect changes, and no SQLite source reads
        // content_hash). See ContentHashMode.
        Self::check_path_with_related_mode(
            path,
            related_paths,
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    /// Fingerprint a Devin Desktop ACP stream together with every CLI database
    /// that can resolve its title to a model/session id. A database or WAL
    /// change can alter a cached Desktop message even when the NDJSON stream is
    /// untouched, so the lookup inputs must be watched as related files.
    pub(crate) fn check_devin_desktop_path_samples_only(
        path: &Path,
        devin_db_paths: &[PathBuf],
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        let related_paths = devin_db_paths
            .iter()
            .enumerate()
            .flat_map(|(index, db_path)| {
                let prefix = format!("devin-cli-db-{index}");
                [
                    (prefix.clone(), db_path.clone()),
                    (format!("{prefix}-wal"), append_path_suffix(db_path, "-wal")),
                ]
            });
        Self::check_path_with_related_mode(
            path,
            related_paths,
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    pub(crate) fn check_jcode_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_jcode_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_jcode_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let related_paths = std::iter::once((
            ".journal.jsonl".to_string(),
            crate::sessions::jcode::jcode_journal_path(path),
        ));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_roo_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_roo_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    pub(crate) fn check_cline_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        let related_paths = if crate::sessions::cline::is_cline_cli_messages_path(path) {
            std::iter::once((
                "manifest.json".to_string(),
                crate::sessions::cline::cline_cli_manifest_path(path),
            ))
            .collect::<Vec<_>>()
        } else {
            let history = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("api_conversation_history.json");
            vec![("api_conversation_history.json".to_string(), history)]
        };
        Self::check_path_with_related_mode(
            path,
            related_paths,
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    fn check_roo_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let history = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("api_conversation_history.json");
        let related_paths = std::iter::once(("api_conversation_history.json".to_string(), history));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }
}
