use super::super::cache::load_or_parse_source_with_fingerprint;
use super::super::registry::{CachePolicy, ClientParserDef, MergePolicy};
use super::PricedContext;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn cached_files(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let parser = spec.parser.expect("cached file parser");
    let CachePolicy::Sampled(fingerprint) = spec.cache else {
        unreachable!("cached file parser without a sampled cache policy");
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
        .collect();
    context.drain(spec, outcomes);
}

pub(crate) fn uncached_files(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let parser = spec.parser.expect("uncached file parser");
    let mut messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| parser(path))
        .collect::<Vec<_>>();
    for message in &mut messages {
        context.price(message, spec.pricing);
    }
    match spec.merge {
        MergePolicy::Append => {}
        MergePolicy::Dedup => {
            let mut seen = HashSet::new();
            messages.retain(|message| {
                message
                    .dedup_key
                    .as_ref()
                    .is_none_or(|key| seen.insert(key.clone()))
            });
        }
        _ => unreachable!("special merge policy reached the common uncached parser"),
    }
    context.messages.extend(messages);
}
