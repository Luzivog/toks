use super::super::cache::{load_or_parse_source, load_or_parse_sqlite_source};
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::{merge_workbuddy_messages, partition_workbuddy_paths};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub(crate) fn workbuddy(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("WorkBuddy parser");
    let identity = context.identity(spec);
    let (detailed_paths, fallback_paths) =
        partition_workbuddy_paths(context.scan.get(spec.scan_bucket));
    let detailed = detailed_paths
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                parser,
            )
        })
        .collect::<Vec<_>>();
    let fallback = fallback_paths
        .par_iter()
        .map(|path| {
            load_or_parse_sqlite_source(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                parser,
            )
        })
        .collect::<Vec<_>>();
    let mut detailed_messages = Vec::new();
    let mut fallback_messages = Vec::new();
    for outcome in detailed {
        detailed_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    for outcome in fallback {
        fallback_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    context.messages.extend(merge_workbuddy_messages(
        detailed_messages,
        fallback_messages,
    ));
}
