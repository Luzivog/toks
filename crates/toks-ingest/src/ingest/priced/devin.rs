use super::super::cache::{
    load_or_parse_source_with_fingerprint_context, load_or_parse_sqlite_source,
};
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::{
    devin_desktop_lookup_cell_for_snapshot, message_cache, sessions, should_keep_deduped_message,
    DevinDesktopLookupCache,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn devin_cli(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let parser = spec.parser.expect("Devin CLI SQLite parser");
    let identity = context.identity(spec);
    let outcomes = context
        .scan
        .devin_dbs
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
    let mut seen = HashSet::new();
    for outcome in outcomes {
        for message in outcome
            .messages
            .into_iter()
            .filter(|message| should_keep_deduped_message(&mut seen, message))
        {
            context
                .devin_cli_session_ids
                .insert(message.session_id.clone());
            context.messages.push(message);
        }
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
}

pub(crate) fn devin_desktop(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let identity = context.identity(spec);
    let lookups = DevinDesktopLookupCache::default();
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint_context(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                |path, cached| {
                    message_cache::SourceFingerprint::check_devin_desktop_path_samples_only(
                        path,
                        &context.scan.devin_dbs,
                        cached,
                    )
                },
                |path, fingerprint| {
                    if let Some(fingerprint) = fingerprint {
                        let lookup_cell = devin_desktop_lookup_cell_for_snapshot(
                            &lookups,
                            &context.scan.devin_dbs,
                            fingerprint,
                        );
                        let lookup = lookup_cell.get_or_init(|| {
                            sessions::devin::load_devin_desktop_session_lookup(
                                &context.scan.devin_dbs,
                            )
                        });
                        sessions::devin::parse_devin_desktop_ndjson_with_lookup(path, lookup)
                    } else {
                        sessions::devin::parse_devin_desktop_ndjson_with_lookup(
                            path,
                            &sessions::devin::load_devin_desktop_session_lookup(
                                &context.scan.devin_dbs,
                            ),
                        )
                    }
                },
            )
        })
        .collect::<Vec<_>>();
    for outcome in outcomes {
        context.messages.extend(
            outcome
                .messages
                .into_iter()
                .filter(|message| !context.devin_cli_session_ids.contains(&message.session_id)),
        );
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
}
