use super::super::cache::{
    load_or_parse_codex_source, load_or_parse_source_with_fingerprint_retaining_history,
};
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::{message_cache, sessions, should_keep_deduped_message};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;
use std::path::PathBuf;

pub(crate) fn claude(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let identity = context.identity(spec);
    let claude_home = PathBuf::from(context.home_dir);
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint_retaining_history(
                identity,
                path,
                &context.source_cache,
                context.pricing,
                sessions::claudecode::dedup_key_is_globally_stable,
                |path, cached| {
                    message_cache::SourceFingerprint::check_claude_code_path_with_home_samples_only(
                        path,
                        cached,
                        Some(&claude_home),
                    )
                },
                |path| sessions::claudecode::parse_claude_file_with_home(path, Some(&claude_home)),
            )
        })
        .collect::<Vec<_>>();

    let mut seen = HashSet::new();
    for outcome in outcomes {
        context
            .messages
            .extend(outcome.messages.into_iter().filter(|message| {
                message
                    .dedup_key
                    .as_ref()
                    .is_none_or(|key| key.is_empty() || seen.insert(key.clone()))
            }));
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
}

pub(crate) fn codex(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| {
            (
                path.clone(),
                load_or_parse_codex_source(
                    path,
                    &context.source_cache,
                    context.pricing,
                    &context.headless_roots,
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for (path, outcome) in outcomes {
        context.messages.extend(
            outcome
                .messages
                .into_iter()
                .filter(|message| should_keep_deduped_message(&mut seen, message)),
        );
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        } else if outcome.invalidate_cache {
            context.remove_cache(spec.client, &path);
        }
    }
}
