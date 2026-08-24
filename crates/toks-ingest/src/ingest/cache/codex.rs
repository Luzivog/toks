use super::codex_entry::{build_codex_cache_entry, finalize_codex_messages, parse_full_log_source};
use super::CachedParseOutcome;
use crate::{is_headless_path, message_cache, pricing, sessions, ClientId};
use std::path::{Path, PathBuf};

pub(crate) fn load_or_parse_codex_source(
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    headless_roots: &[PathBuf],
) -> CachedParseOutcome {
    let identity = message_cache::CacheIdentity::for_client(ClientId::Codex);
    let is_headless = is_headless_path(path, headless_roots);
    let cached = source_cache.get(identity, path);
    if cached.is_none() {
        // The post-parse cache build computes the authoritative fingerprint
        // after reading the file. Avoid hashing an uncached source here
        // only to discard that digest before parsing it.
        return parse_full_log_source(path, pricing, is_headless);
    }
    let Some(fingerprint_status) =
        message_cache::SourceFingerprint::check_path(path, cached.map(|entry| &entry.fingerprint))
    else {
        return parse_full_log_source(path, pricing, is_headless);
    };
    let fingerprint = match fingerprint_status {
        message_cache::FingerprintStatus::Unchanged => cached
            .expect("an uncached source always builds a complete fingerprint")
            .fingerprint
            .clone(),
        message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
    };
    let fallback_timestamp = sessions::utils::file_modified_timestamp_ms(path);

    if let Some(cached) = cached {
        let reparse_from_start = |invalidate_cache: bool| {
            let mut outcome = parse_full_log_source(path, pricing, is_headless);
            outcome.invalidate_cache = invalidate_cache && outcome.cache_entry.is_none();
            outcome
        };

        if cached.fingerprint == fingerprint {
            if message_cache::codex_cache_entry_matches_fingerprint(cached, &fingerprint) {
                return CachedParseOutcome {
                    messages: finalize_codex_messages(
                        cached.messages.clone(),
                        pricing,
                        is_headless,
                        &cached.fallback_timestamp_indices,
                        fallback_timestamp,
                    ),
                    cache_entry: None,
                    invalidate_cache: false,
                };
            }

            return reparse_from_start(true);
        }

        if let Some(codex_incremental) = cached.codex_incremental.as_ref() {
            if fingerprint.size > codex_incremental.consumed_offset
                && message_cache::codex_prefix_matches(path, codex_incremental)
            {
                let parsed = sessions::codex::parse_codex_file_incremental(
                    path,
                    codex_incremental.consumed_offset,
                    codex_incremental.state.clone(),
                );
                if parsed.parse_succeeded && !parsed.unresolved_model_events {
                    let mut raw_messages = cached.messages.clone();
                    let mut fallback_timestamp_indices = cached.fallback_timestamp_indices.clone();
                    let existing_len = raw_messages.len();
                    fallback_timestamp_indices.extend(
                        parsed
                            .fallback_timestamp_indices
                            .iter()
                            .map(|index| existing_len + index),
                    );
                    raw_messages.extend(parsed.messages.clone());
                    let cache_entry = build_codex_cache_entry(
                        path,
                        raw_messages.clone(),
                        parsed.consumed_offset,
                        parsed.state,
                        fallback_timestamp_indices.clone(),
                    );
                    if let Some(cache_entry) = cache_entry {
                        let messages = finalize_codex_messages(
                            raw_messages,
                            pricing,
                            is_headless,
                            &fallback_timestamp_indices,
                            fallback_timestamp,
                        );

                        return CachedParseOutcome {
                            messages,
                            cache_entry: Some(cache_entry),
                            invalidate_cache: false,
                        };
                    }
                }
            }
        }

        return reparse_from_start(true);
    }

    unreachable!("uncached Codex sources return before fingerprint validation")
}
