use super::related::{primary_fingerprint_matches, related_fingerprint_metadata_matches};
use super::{FingerprintStatus, SourceFingerprint};
use crate::message_cache::shard::ContentHashMode;
use std::path::{Path, PathBuf};

impl SourceFingerprint {
    pub(crate) fn check_path(path: &Path, cached: Option<&Self>) -> Option<FingerprintStatus> {
        Self::check_path_with_related(path, std::iter::empty(), cached)
    }

    /// Check a non-Codex source without rebuilding its write-only whole-file
    /// hash when metadata or samples changed. Codex uses `check_path` because
    /// its incremental resume state compares the full content hash; generic
    /// parsers only need the bounded samples for invalidation.
    pub(crate) fn check_path_samples_only(
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

    pub(super) fn check_path_with_related<I>(
        path: &Path,
        related_paths: I,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        Self::check_path_with_related_mode(path, related_paths, cached, ContentHashMode::Full)
    }

    pub(super) fn check_path_with_related_mode<I>(
        path: &Path,
        related_paths: I,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        Self::check_path_with_related_mode_and_primary(path, related_paths, cached, mode, None)
    }

    pub(super) fn check_path_with_related_mode_and_primary<I>(
        path: &Path,
        related_paths: I,
        cached: Option<&Self>,
        mode: ContentHashMode,
        primary_matches: Option<bool>,
    ) -> Option<FingerprintStatus>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        let related_paths: Vec<(String, PathBuf)> = related_paths.into_iter().collect();
        let cache_hit = cached.is_some_and(|fingerprint| {
            primary_matches
                .unwrap_or_else(|| primary_fingerprint_matches(path, fingerprint).unwrap_or(false))
                && related_fingerprint_metadata_matches(&related_paths, fingerprint)
                    .unwrap_or(false)
        });
        if cache_hit {
            return Some(FingerprintStatus::Unchanged);
        }

        Self::from_path_with_related_mode(path, related_paths, mode).map(FingerprintStatus::Changed)
    }
}
