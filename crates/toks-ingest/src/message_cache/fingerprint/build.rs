use super::{RelatedFileFingerprint, SourceFingerprint};
#[cfg(test)]
use crate::message_cache::shard::append_path_suffix;
use crate::message_cache::shard::{file_fingerprint_parts, ContentHashMode};
use std::path::{Path, PathBuf};

impl SourceFingerprint {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        Self::from_path_with_related(path, std::iter::empty())
    }

    #[cfg(test)]
    pub(crate) fn from_sqlite_path(path: &Path) -> Option<Self> {
        let related_paths = ["-wal"]
            .into_iter()
            .map(|suffix| (suffix.to_string(), append_path_suffix(path, suffix)));
        Self::from_path_with_related_mode(path, related_paths, ContentHashMode::SamplesOnly)
    }

    /// Fingerprint for a Jcode session snapshot and its append-only journal
    /// sidecar. Jcode persists recent changes in `session_*.journal.jsonl`
    /// until the next checkpoint rewrites the snapshot, so the source-message
    /// cache must invalidate when either file changes.
    #[cfg(test)]
    pub(crate) fn from_jcode_path(path: &Path) -> Option<Self> {
        let related_paths = std::iter::once((
            ".journal.jsonl".to_string(),
            crate::sessions::jcode::jcode_journal_path(path),
        ));
        Self::from_path_with_related(path, related_paths)
    }

    /// Fingerprint for a Roo-family task (`ui_messages.json`) and its sibling
    /// `api_conversation_history.json`. `parse_roo_kilo_file` reads the history
    /// sibling for the model and agent, so a history-only rewrite (the UI file
    /// unchanged) must still invalidate the cache or reports keep stale
    /// model/agent/pricing.
    #[cfg(test)]
    pub(crate) fn from_roo_path(path: &Path) -> Option<Self> {
        let history = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("api_conversation_history.json");
        let related_paths = std::iter::once(("api_conversation_history.json".to_string(), history));
        Self::from_path_with_related(path, related_paths)
    }

    /// Fingerprint for a Claude Code JSONL file that may have a sibling `.meta.json`
    /// sidecar. When the sidecar appears or changes (e.g. after a Claude Code upgrade),
    /// the fingerprint changes and the cache invalidates.
    #[cfg(test)]
    pub(crate) fn from_claude_code_path_with_home(
        path: &Path,
        home_dir: Option<&Path>,
    ) -> Option<Self> {
        let mut related = Vec::new();

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let meta_filename = format!("{}.meta.json", stem);
            related.push((".meta.json".to_string(), path.with_file_name(meta_filename)));
        }

        if let Some(variant_path) = crate::cc_mirror::variant_file_for_session_path(path, home_dir)
        {
            related.push(("cc-mirror/variant.json".to_string(), variant_path));
        }
        for (index, parent_path) in
            crate::sessions::claudecode::parent_session_paths_for_cache(path)
                .into_iter()
                .enumerate()
        {
            related.push((format!("parent-session-{index}.jsonl"), parent_path));
        }

        Self::from_path_with_related(path, related)
    }

    /// Fingerprint for a Grok source and every file or directory read by its
    /// parser for rollup and session metadata. Unified-log parsing also reads
    /// metadata across the complete sessions tree.
    #[cfg(test)]
    pub(crate) fn from_grok_path(path: &Path) -> Option<Self> {
        Self::from_path_with_related(path, crate::sessions::grok::grok_related_paths(path))
    }

    /// Fingerprint for a Kiro source file. IDE sessions consume a sibling
    /// `messages.jsonl`, while CLI `*.json` headers consume same-stem `*.jsonl`.
    /// Global-storage and `.chat` snapshots are self-contained.
    #[cfg(test)]
    pub(crate) fn from_kiro_path(path: &Path) -> Option<Self> {
        let Some(messages) = crate::sessions::kiro::kiro_related_messages_path(path) else {
            return Self::from_path(path);
        };
        let related_paths = std::iter::once(("messages.jsonl".to_string(), messages));
        Self::from_path_with_related(path, related_paths)
    }

    #[cfg(test)]
    pub(crate) fn from_droid_path(path: &Path) -> Option<Self> {
        let Some(jsonl) = crate::sessions::droid::droid_jsonl_path(path) else {
            return Self::from_path(path);
        };
        let related_paths = std::iter::once(("session.jsonl".to_string(), jsonl));
        Self::from_path_with_related(path, related_paths)
    }

    #[cfg(test)]
    pub(crate) fn from_kimi_path(path: &Path) -> Option<Self> {
        if crate::sessions::kimi::is_kimi_code_path(path) {
            return Self::from_path(path);
        }
        let Some(config) = crate::sessions::kimi::kimi_config_path(path) else {
            return Self::from_path(path);
        };
        let related_paths = std::iter::once(("config.json".to_string(), config));
        Self::from_path_with_related(path, related_paths)
    }

    pub(super) fn from_path_with_related<I>(path: &Path, related_paths: I) -> Option<Self>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        Self::from_path_with_related_mode(path, related_paths, ContentHashMode::Full)
    }

    pub(super) fn from_path_with_related_mode<I>(
        path: &Path,
        related_paths: I,
        mode: ContentHashMode,
    ) -> Option<Self>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        let (size, modified_ns, sample_hashes, content_hash) = file_fingerprint_parts(path, mode)?;
        let mut related_files: Vec<RelatedFileFingerprint> = related_paths
            .into_iter()
            .map(|(suffix, related_path)| {
                RelatedFileFingerprint::from_path(suffix, &related_path, mode)
            })
            .collect::<Option<_>>()?;
        related_files.sort_by(|left, right| left.suffix.cmp(&right.suffix));

        Some(Self {
            size,
            modified_ns,
            sample_hashes,
            content_hash,
            related_files,
        })
    }
}
