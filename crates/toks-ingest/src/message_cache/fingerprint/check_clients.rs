use super::related::{cached_claude_parent_paths, primary_fingerprint_matches};
use super::{FingerprintStatus, SourceFingerprint};
use crate::message_cache::shard::ContentHashMode;
use std::path::Path;

impl SourceFingerprint {
    pub(crate) fn check_claude_code_path_with_home_samples_only(
        path: &Path,
        cached: Option<&Self>,
        home_dir: Option<&Path>,
    ) -> Option<FingerprintStatus> {
        Self::check_claude_code_path_with_home_mode(
            path,
            cached,
            home_dir,
            ContentHashMode::SamplesOnly,
        )
    }

    fn check_claude_code_path_with_home_mode(
        path: &Path,
        cached: Option<&Self>,
        home_dir: Option<&Path>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let mut related = Vec::new();

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let meta_filename = format!("{}.meta.json", stem);
            related.push((".meta.json".to_string(), path.with_file_name(meta_filename)));
        }

        if let Some(variant_path) = crate::cc_mirror::variant_file_for_session_path(path, home_dir)
        {
            related.push(("cc-mirror/variant.json".to_string(), variant_path));
        }

        let primary_matches =
            cached.and_then(|fingerprint| primary_fingerprint_matches(path, fingerprint));
        let parent_paths = cached
            .filter(|_| primary_matches == Some(true))
            .map(cached_claude_parent_paths)
            .unwrap_or_else(|| {
                crate::sessions::claudecode::parent_session_paths_for_cache(path)
                    .into_iter()
                    .enumerate()
                    .map(|(index, parent_path)| {
                        (format!("parent-session-{index}.jsonl"), parent_path)
                    })
                    .collect()
            });
        related.extend(parent_paths);

        Self::check_path_with_related_mode_and_primary(path, related, cached, mode, primary_matches)
    }

    pub(crate) fn check_grok_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_grok_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_grok_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let related_paths = crate::sessions::grok::grok_related_paths(path);
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_kiro_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_kiro_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_kiro_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let Some(messages) = crate::sessions::kiro::kiro_related_messages_path(path) else {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        };
        let related_paths = std::iter::once(("messages.jsonl".to_string(), messages));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_droid_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_droid_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_droid_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let Some(jsonl) = crate::sessions::droid::droid_jsonl_path(path) else {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        };
        let related_paths = std::iter::once(("session.jsonl".to_string(), jsonl));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_kimi_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_kimi_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    /// Stats are append-only JSONL; use bounded samples to avoid hashing a
    /// growing daily log on every warm scan.
    pub(crate) fn check_reasonix_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_path_with_related_mode(
            path,
            std::iter::empty(),
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    fn check_kimi_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        if crate::sessions::kimi::is_kimi_code_path(path) {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        }
        let Some(config) = crate::sessions::kimi::kimi_config_path(path) else {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        };
        let related_paths = std::iter::once(("config.json".to_string(), config));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }
}
