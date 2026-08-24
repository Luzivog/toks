use super::super::cache::{load_or_parse_source_with_fingerprint, load_or_parse_sqlite_source};
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::{
    message_cache, parse_hermes_sqlite_with_pricing, sessions, should_keep_deduped_message,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn zcode(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let identity = context.identity(spec);
    if let Some(path) = &context.scan.zcode_db {
        let outcome = load_or_parse_sqlite_source(
            identity,
            path,
            &context.source_cache,
            context.pricing,
            sessions::zcode::parse_zcode_sqlite,
        );
        context.messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    let mut messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| sessions::zcode::parse_zcode_file(path))
        .collect::<Vec<_>>();
    for message in &mut messages {
        context.price(message, spec.pricing);
    }
    context.messages.extend(messages);
}

pub(crate) fn kilo(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let Some(path) = &context.scan.kilo_db else {
        return;
    };
    let parser = spec.parser.expect("Kilo SQLite parser");
    let mut messages = parser(path);
    for message in &mut messages {
        context.price(message, spec.pricing);
    }
    context.messages.extend(messages);
}

pub(crate) fn hermes(context: &mut PricedContext<'_>, _spec: &'static ClientParserDef) {
    let mut seen = HashSet::new();
    for path in context.scan.hermes_db_paths() {
        context.messages.extend(
            parse_hermes_sqlite_with_pricing(&path, context.pricing)
                .into_iter()
                .filter(|message| should_keep_deduped_message(&mut seen, message)),
        );
    }
}

pub(crate) fn goose(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let Some(path) = &context.scan.goose_db else {
        return;
    };
    let parser = spec.parser.expect("Goose SQLite parser");
    let mut messages = parser(path);
    for message in &mut messages {
        context.price(message, spec.pricing);
    }
    context.messages.extend(messages);
}

pub(crate) fn zed(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Zed SQLite parser");
    let identity = context.identity(spec);
    for path in context.scan.zed_db_paths() {
        let outcome = load_or_parse_sqlite_source(
            identity,
            &path,
            &context.source_cache,
            context.pricing,
            parser,
        );
        context.messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
}

pub(crate) fn kiro(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Kiro file parser");
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
                message_cache::SourceFingerprint::check_kiro_path_samples_only,
                parser,
            )
        })
        .collect::<Vec<_>>();
    let mut file_messages = Vec::new();
    for outcome in outcomes {
        file_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    context
        .messages
        .extend(sessions::kiro::suppress_snapshots_covered_by_executions(
            file_messages,
        ));

    if let Some(path) = &context.scan.kiro_db {
        let mut database_messages = sessions::kiro::parse_kiro_sqlite(path);
        for message in &mut database_messages {
            context.price(message, spec.pricing);
        }
        context.messages.extend(database_messages);
    }
}
