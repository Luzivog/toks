use super::super::cache::{
    apply_pricing_to_messages, load_or_parse_source_with_fingerprint,
    load_or_parse_source_with_fingerprint_and_policy, HistoryRetention,
};
use super::super::registry::{CachePolicy, ClientParserDef};
use super::PricedContext;
use crate::{message_cache, sessions};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub(crate) fn gemini(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let identity = context.identity(spec);
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            (
                path.clone(),
                load_or_parse_source_with_fingerprint_and_policy(
                    identity,
                    path,
                    &context.source_cache,
                    context.pricing,
                    HistoryRetention::LiveFileOnly,
                    message_cache::SourceFingerprint::check_path_samples_only,
                    |path, _| {
                        let parsed = sessions::gemini::parse_gemini_file_with_cache_status(path);
                        (parsed.messages, parsed.cacheable)
                    },
                ),
            )
        })
        .collect::<Vec<_>>();
    for (path, outcome) in outcomes {
        context.messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        } else if outcome.invalidate_cache {
            context.remove_cache(spec.client, &path);
        }
    }
}

pub(crate) fn kimi(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let CachePolicy::Sampled(fingerprint) = spec.cache else {
        unreachable!("Kimi fingerprint policy");
    };
    let identity = context.identity(spec);
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            let parser = if sessions::kimi::is_kimi_code_path(path) {
                sessions::kimi::parse_kimi_code_file
            } else {
                sessions::kimi::parse_kimi_file
            };
            load_or_parse_source_with_fingerprint(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                fingerprint,
                parser,
            )
        })
        .collect();
    context.drain_append(outcomes);
}

pub(crate) fn grok(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Grok file parser");
    let CachePolicy::Sampled(fingerprint) = spec.cache else {
        unreachable!("Grok fingerprint policy");
    };
    let identity = context.identity(spec);
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                fingerprint,
                parser,
            )
        })
        .collect::<Vec<_>>();
    let mut messages = Vec::new();
    for outcome in outcomes {
        messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    let mut selected = sessions::grok::prefer_unified_log_messages(messages);
    apply_pricing_to_messages(&mut selected, context.pricing);
    context.messages.extend(selected);
}
