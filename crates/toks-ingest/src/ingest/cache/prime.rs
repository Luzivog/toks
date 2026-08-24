use super::{apply_pricing_to_messages, cached_messages, CachedParseOutcome};
use crate::{message_cache, pricing, sessions, ClientId, UnifiedMessage};
use std::path::Path;

fn uncached_prime_outcome(
    mut messages: Vec<UnifiedMessage>,
    accounting: sessions::prime_agent::PrimeFileAccounting,
    pricing: Option<&pricing::PricingService>,
) -> (
    CachedParseOutcome,
    sessions::prime_agent::PrimeFileAccounting,
) {
    apply_pricing_to_messages(&mut messages, pricing);
    (
        CachedParseOutcome {
            messages,
            cache_entry: None,
            invalidate_cache: false,
        },
        accounting,
    )
}

fn parse_stable_prime_source(
    path: &Path,
    identity: message_cache::CacheIdentity,
    mut fingerprint_before: message_cache::SourceFingerprint,
    pricing: Option<&pricing::PricingService>,
) -> (
    CachedParseOutcome,
    sessions::prime_agent::PrimeFileAccounting,
) {
    const MAX_STABLE_PARSE_ATTEMPTS: usize = 2;

    let mut last_parse = None;
    for _ in 0..MAX_STABLE_PARSE_ATTEMPTS {
        #[cfg(test)]
        sessions::prime_agent::run_stable_parse_test_hook(path);

        // Both views come from this one decoded record stream. Exact hashes
        // on either side ensure that the pair is only cached when the bytes
        // stayed at the fingerprint under which the entry is stored.
        let parsed = sessions::prime_agent::parse_prime_agent_file_with_accounting(path);
        let Some(fingerprint_after) = message_cache::SourceFingerprint::from_path(path) else {
            return uncached_prime_outcome(parsed.0, parsed.1, pricing);
        };
        if fingerprint_after == fingerprint_before {
            let (mut messages, accounting) = parsed;
            let cache_entry = (!messages.is_empty()).then(|| {
                message_cache::CachedSourceEntry::new(
                    identity,
                    path,
                    fingerprint_after,
                    messages.clone(),
                    Vec::new(),
                    None,
                )
                .with_prime_accounting(accounting.clone())
            });
            apply_pricing_to_messages(&mut messages, pricing);
            return (
                CachedParseOutcome {
                    messages,
                    cache_entry,
                    invalidate_cache: false,
                },
                accounting,
            );
        }

        fingerprint_before = fingerprint_after;
        last_parse = Some(parsed);
    }

    // A continuously rewritten file still yields a coherent messages +
    // accounting pair from one pass, but no cache entry may claim that pair
    // belongs to either exact fingerprint observed around the read.
    let (messages, accounting) = last_parse.expect("the retry bound is non-zero");
    uncached_prime_outcome(messages, accounting, pricing)
}

pub(crate) fn load_or_parse_prime_source(
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
) -> (
    CachedParseOutcome,
    sessions::prime_agent::PrimeFileAccounting,
) {
    let identity = message_cache::CacheIdentity::for_client(ClientId::PrimeAgent);
    let cached = source_cache.get(identity, path);
    let Some(fingerprint_status) =
        message_cache::SourceFingerprint::check_path(path, cached.map(|entry| &entry.fingerprint))
    else {
        let (messages, accounting) =
            sessions::prime_agent::parse_prime_agent_file_with_accounting(path);
        return uncached_prime_outcome(messages, accounting, pricing);
    };

    let mut fingerprint = match fingerprint_status {
        message_cache::FingerprintStatus::Unchanged => cached
            .expect("an uncached source always builds a complete fingerprint")
            .fingerprint
            .clone(),
        message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
    };

    if let Some(cached) = cached {
        if cached.fingerprint == fingerprint && !cached.messages.is_empty() {
            if let Some(accounting) = cached.prime_accounting.as_ref() {
                // Prime's accounting is byte-coupled to its messages. Warm
                // v5 scans therefore hash the complete transcript before a
                // hit, while still avoiding JSON decode and accounting walk.
                match message_cache::SourceFingerprint::from_path(path) {
                    Some(refreshed) if refreshed == cached.fingerprint => {
                        return (
                            CachedParseOutcome {
                                messages: cached_messages(cached, pricing),
                                cache_entry: None,
                                invalidate_cache: false,
                            },
                            accounting.clone(),
                        );
                    }
                    Some(refreshed) => fingerprint = refreshed,
                    None => {
                        let (messages, accounting) =
                            sessions::prime_agent::parse_prime_agent_file_with_accounting(path);
                        return uncached_prime_outcome(messages, accounting, pricing);
                    }
                }
            } else {
                // Version-4 entries already contain valid messages but predate
                // Prime accounting metadata. Decode just the accounting view
                // once, but never combine it with those messages until the
                // fingerprint is revalidated with a full content hash: the
                // file can change between the first bounded-sample check and
                // this second transcript read, including outside sample windows.
                #[cfg(test)]
                sessions::prime_agent::run_accounting_backfill_test_hook(path);
                let accounting =
                    sessions::prime_agent::analyze_prime_agent_accounting(path, &cached.messages);
                match message_cache::SourceFingerprint::from_path(path) {
                    Some(refreshed) if refreshed == fingerprint => {
                        return (
                            CachedParseOutcome {
                                messages: cached_messages(cached, pricing),
                                cache_entry: Some(
                                    cached.clone().with_prime_accounting(accounting.clone()),
                                ),
                                invalidate_cache: false,
                            },
                            accounting,
                        );
                    }
                    Some(refreshed) => fingerprint = refreshed,
                    None => {
                        let (messages, accounting) =
                            sessions::prime_agent::parse_prime_agent_file_with_accounting(path);
                        return uncached_prime_outcome(messages, accounting, pricing);
                    }
                }
            }
        }
    }

    parse_stable_prime_source(path, identity, fingerprint, pricing)
}
