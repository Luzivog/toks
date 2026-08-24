use super::{apply_pricing_to_messages, cached_messages, CachedParseOutcome};
use crate::{message_cache, pricing, UnifiedMessage};
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) enum HistoryRetention {
    /// The live file is the whole truth. Anything it no longer contains
    /// leaves the totals — correct for a source whose file content is a
    /// faithful record of what the client did.
    LiveFileOnly,
    /// Carry forward messages this exact file was previously observed to
    /// contain. For clients that rewrite a transcript in place.
    RetainObserved {
        /// Decides, per dedup key, whether the message may be carried
        /// forward. A retained copy outlives the bytes that produced it,
        /// so it has to keep collapsing against a live copy of the same
        /// message written anywhere else; a key that is only unique
        /// within one file cannot do that and must be dropped instead.
        /// The lane owning the key format supplies this.
        key_is_globally_stable: fn(&str) -> bool,
    },
}

/// Merge the messages a previous scan recorded for this exact file back
/// into a fresh parse of it.
///
/// Within this entry the live file stays authoritative for everything it
/// still contains: a key present on both sides keeps the freshly parsed
/// message, so a corrected re-parse still wins and nothing is frozen at a
/// stale value. Only keys the file no longer carries are carried forward.
/// (Across entries the Claude lane is first-wins on lexical path order, so
/// a retained copy in an earlier-sorting file still beats a live copy of
/// the same key in a later one.)
///
/// Messages without a dedup key are never retained. The key is what lets a
/// later scan recognise the message as already-seen; re-emitting an
/// unkeyed one would double count it the moment the file regained it. Keys
/// that `key_is_globally_stable` rejects are dropped for the same reason:
/// they would never collapse against a live copy elsewhere.
fn retain_observed_messages(
    parsed: &mut Vec<UnifiedMessage>,
    cached: &[UnifiedMessage],
    key_is_globally_stable: fn(&str) -> bool,
) {
    let mut seen: HashSet<String> = parsed
        .iter()
        .filter_map(|message| message.dedup_key.clone())
        .collect();

    for message in cached {
        let Some(key) = message.dedup_key.as_ref() else {
            continue;
        };
        if !key_is_globally_stable(key) {
            continue;
        }
        if seen.insert(key.clone()) {
            parsed.push(message.clone());
        }
    }
}

pub(crate) fn load_or_parse_source_with_fingerprint_and_policy<F, FingerprintFn>(
    identity: message_cache::CacheIdentity,
    path: &Path,
    source_cache: &message_cache::SourceMessageCache,
    pricing: Option<&pricing::PricingService>,
    history: HistoryRetention,
    fingerprint_from_path: FingerprintFn,
    parse: F,
) -> CachedParseOutcome
where
    F: Fn(&Path, Option<&message_cache::SourceFingerprint>) -> (Vec<UnifiedMessage>, bool),
    FingerprintFn: Fn(
        &Path,
        Option<&message_cache::SourceFingerprint>,
    ) -> Option<message_cache::FingerprintStatus>,
{
    let cached = source_cache.get(identity, path);
    let Some(fingerprint_status) =
        fingerprint_from_path(path, cached.map(|entry| &entry.fingerprint))
    else {
        let (mut messages, _) = parse(path, None);
        apply_pricing_to_messages(&mut messages, pricing);
        return CachedParseOutcome {
            messages,
            cache_entry: None,
            invalidate_cache: false,
        };
    };

    let fingerprint = match fingerprint_status {
        message_cache::FingerprintStatus::Unchanged => {
            let Some(cached) = cached else {
                unreachable!("an uncached source always builds a complete fingerprint")
            };
            if !cached.messages.is_empty() {
                return CachedParseOutcome {
                    messages: cached_messages(cached, pricing),
                    cache_entry: None,
                    invalidate_cache: false,
                };
            }
            cached.fingerprint.clone()
        }
        message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
    };

    if let Some(cached) = cached {
        if cached.fingerprint == fingerprint && !cached.messages.is_empty() {
            return CachedParseOutcome {
                messages: cached_messages(cached, pricing),
                cache_entry: None,
                invalidate_cache: false,
            };
        }
    }

    let (mut messages, cacheable) = parse(path, Some(&fingerprint));
    // Reaching here means the file changed under a cache entry we still
    // hold. For a source that rewrites transcripts in place that is not
    // only "new content appeared" — it can also be "already-published
    // messages disappeared", and recomputing purely from the live bytes
    // would retire them from history (#994). Only merge when the parse is
    // cacheable: an untrustworthy parse must not be used to synthesise an
    // entry, and the caller invalidates on that path anyway.
    if let HistoryRetention::RetainObserved {
        key_is_globally_stable,
    } = history
    {
        if cacheable {
            if let Some(cached) = cached {
                retain_observed_messages(&mut messages, &cached.messages, key_is_globally_stable);
            }
        }
    }
    let cache_entry = if messages.is_empty() || !cacheable {
        None
    } else {
        Some(message_cache::CachedSourceEntry::new(
            identity,
            path,
            fingerprint,
            messages.clone(),
            Vec::new(),
            None,
        ))
    };
    apply_pricing_to_messages(&mut messages, pricing);

    CachedParseOutcome {
        messages,
        cache_entry,
        invalidate_cache: !cacheable,
    }
}
