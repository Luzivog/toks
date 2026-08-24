use super::{
    load_or_parse_source_with_fingerprint_and_policy, CachedParseOutcome, HistoryRetention,
};
use crate::{message_cache, pricing, UnifiedMessage};
use std::path::Path;

pub(crate) fn load_or_parse_source_with_fingerprint<F, FingerprintFn>(
    identity: message_cache::CacheIdentity,
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    fingerprint_from_path: FingerprintFn,
    parse: F,
) -> CachedParseOutcome
where
    F: Fn(&Path) -> Vec<UnifiedMessage>,
    FingerprintFn: Fn(
        &Path,
        Option<&message_cache::SourceFingerprint>,
    ) -> Option<message_cache::FingerprintStatus>,
{
    load_or_parse_source_with_fingerprint_and_policy(
        identity,
        path,
        source_cache,
        pricing,
        HistoryRetention::LiveFileOnly,
        fingerprint_from_path,
        |path, _| (parse(path), true),
    )
}

/// Same as `load_or_parse_source_with_fingerprint`, for clients that
/// rewrite an existing transcript instead of only appending to it.
///
/// Scoped deliberately rather than made the default. Retention is only
/// sound where a message carries a dedup key that identifies it by
/// content across files, because the retained copy has to collapse
/// against any live copy of the same message elsewhere. Sources keyed by
/// file position or by a per-scan ordinal do not qualify and would double
/// count, and neither do some keys inside an otherwise-qualifying lane —
/// hence `key_is_globally_stable` rather than a blanket per-client
/// promise.
pub(crate) fn load_or_parse_source_with_fingerprint_retaining_history<F, FingerprintFn>(
    identity: message_cache::CacheIdentity,
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    key_is_globally_stable: fn(&str) -> bool,
    fingerprint_from_path: FingerprintFn,
    parse: F,
) -> CachedParseOutcome
where
    F: Fn(&Path) -> Vec<UnifiedMessage>,
    FingerprintFn: Fn(
        &Path,
        Option<&message_cache::SourceFingerprint>,
    ) -> Option<message_cache::FingerprintStatus>,
{
    load_or_parse_source_with_fingerprint_and_policy(
        identity,
        path,
        source_cache,
        pricing,
        HistoryRetention::RetainObserved {
            key_is_globally_stable,
        },
        fingerprint_from_path,
        |path, _| (parse(path), true),
    )
}

pub(crate) fn load_or_parse_source_with_fingerprint_context<F, FingerprintFn>(
    identity: message_cache::CacheIdentity,
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    fingerprint_from_path: FingerprintFn,
    parse: F,
) -> CachedParseOutcome
where
    F: Fn(&Path, Option<&message_cache::SourceFingerprint>) -> Vec<UnifiedMessage>,
    FingerprintFn: Fn(
        &Path,
        Option<&message_cache::SourceFingerprint>,
    ) -> Option<message_cache::FingerprintStatus>,
{
    load_or_parse_source_with_fingerprint_and_policy(
        identity,
        path,
        source_cache,
        pricing,
        HistoryRetention::LiveFileOnly,
        fingerprint_from_path,
        |path, fingerprint| (parse(path, fingerprint), true),
    )
}

pub(crate) fn load_or_parse_source<F>(
    identity: message_cache::CacheIdentity,
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    parse: F,
) -> CachedParseOutcome
where
    F: Fn(&Path) -> Vec<UnifiedMessage>,
{
    load_or_parse_source_with_fingerprint(
        identity,
        path,
        source_cache,
        pricing,
        message_cache::SourceFingerprint::check_path_samples_only,
        parse,
    )
}

pub(crate) fn load_or_parse_sqlite_source<F>(
    identity: message_cache::CacheIdentity,
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    parse: F,
) -> CachedParseOutcome
where
    F: Fn(&Path) -> Vec<UnifiedMessage>,
{
    load_or_parse_source_with_fingerprint(
        identity,
        path,
        source_cache,
        pricing,
        message_cache::SourceFingerprint::check_sqlite_path,
        parse,
    )
}
