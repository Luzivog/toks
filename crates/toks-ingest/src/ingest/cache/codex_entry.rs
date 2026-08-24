use super::{apply_pricing_to_messages, CachedParseOutcome};
use crate::{apply_headless_agent, message_cache, pricing, sessions, ClientId, UnifiedMessage};
use std::path::Path;

pub(super) fn parse_full_log_source(
    path: &Path,
    pricing: Option<&pricing::PricingService>,
    is_headless: bool,
) -> CachedParseOutcome {
    let fallback_timestamp = sessions::utils::file_modified_timestamp_ms(path);
    let parsed = sessions::codex::parse_codex_file_incremental(
        path,
        0,
        sessions::codex::CodexParseState::default(),
    );
    let messages = finalize_codex_messages(
        parsed.messages.clone(),
        pricing,
        is_headless,
        &parsed.fallback_timestamp_indices,
        fallback_timestamp,
    );
    if !parsed.parse_succeeded {
        return CachedParseOutcome {
            messages,
            cache_entry: None,
            invalidate_cache: false,
        };
    }

    if parsed.unresolved_model_events {
        return CachedParseOutcome {
            messages,
            cache_entry: None,
            invalidate_cache: false,
        };
    }

    let cache_entry = build_codex_cache_entry(
        path,
        parsed.messages,
        parsed.consumed_offset,
        parsed.state,
        parsed.fallback_timestamp_indices,
    );

    CachedParseOutcome {
        messages,
        cache_entry,
        invalidate_cache: false,
    }
}

pub(super) fn finalize_codex_messages(
    mut messages: Vec<UnifiedMessage>,
    pricing: Option<&pricing::PricingService>,
    is_headless: bool,
    fallback_timestamp_indices: &[usize],
    fallback_timestamp: i64,
) -> Vec<UnifiedMessage> {
    for index in fallback_timestamp_indices {
        if let Some(message) = messages.get_mut(*index) {
            message.set_timestamp(fallback_timestamp);
        }
    }
    apply_pricing_to_messages(&mut messages, pricing);
    for message in &mut messages {
        apply_headless_agent(message, is_headless);
    }
    messages
}

pub(super) fn build_codex_cache_entry(
    path: &Path,
    raw_messages: Vec<UnifiedMessage>,
    consumed_offset: u64,
    state: sessions::codex::CodexParseState,
    fallback_timestamp_indices: Vec<usize>,
) -> Option<message_cache::CachedSourceEntry> {
    let fingerprint = message_cache::SourceFingerprint::from_path(path)?;
    if fingerprint.size != consumed_offset {
        return None;
    }

    let codex_incremental = message_cache::build_codex_incremental_cache_with_prefix_hash(
        path,
        consumed_offset,
        state,
        fingerprint.content_hash,
    )?;

    Some(message_cache::CachedSourceEntry::new(
        message_cache::CacheIdentity::for_client(ClientId::Codex),
        path,
        fingerprint,
        raw_messages,
        fallback_timestamp_indices,
        Some(codex_incremental),
    ))
}
