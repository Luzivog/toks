use super::super::cache::{load_or_parse_source, load_or_parse_sqlite_source};
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::sessions;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};

pub(crate) fn open_code(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let identity = context.identity(spec);
    let mut seen = HashSet::new();

    for path in &context.scan.opencode_dbs {
        let outcome = load_or_parse_sqlite_source(
            identity,
            path,
            &context.source_cache,
            context.pricing,
            sessions::opencode::parse_opencode_sqlite,
        );
        context
            .messages
            .extend(outcome.messages.into_iter().filter(|message| {
                message
                    .dedup_key
                    .as_ref()
                    .is_none_or(|key| seen.insert(key.clone()))
            }));
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }

    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                |path| {
                    sessions::opencode::parse_opencode_file(path)
                        .into_iter()
                        .collect()
                },
            )
        })
        .collect::<Vec<_>>();
    for outcome in outcomes {
        context
            .messages
            .extend(outcome.messages.into_iter().filter(|message| {
                message
                    .dedup_key
                    .as_ref()
                    .is_none_or(|key| seen.insert(key.clone()))
            }));
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
}

pub(crate) fn micode(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let identity = context.identity(spec);
    let mut messages = Vec::new();
    let mut indices = HashMap::new();

    for path in &context.scan.micode_dbs {
        let outcome = load_or_parse_sqlite_source(
            identity,
            path,
            &context.source_cache,
            None,
            sessions::micode::parse_micode_sqlite,
        );
        for mut message in outcome.messages {
            context.price(&mut message, spec.pricing);
            if let Some(key) = message.dedup_key.as_ref() {
                if let Some(index) = indices.get(key).copied() {
                    let existing: &mut crate::UnifiedMessage = &mut messages[index];
                    if message.has_authoritative_cost() && !existing.has_authoritative_cost() {
                        existing.cost = message.cost;
                        existing.mark_provider_reported_cost();
                    }
                    continue;
                }
                indices.insert(key.clone(), messages.len());
            }
            messages.push(message);
        }
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    context.messages.extend(messages);
}
