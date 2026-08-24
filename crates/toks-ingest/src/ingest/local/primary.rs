use super::LocalContext;
use crate::ingest::registry::ClientParserDef;
use crate::{
    apply_headless_agent, is_headless_path, sessions, should_keep_deduped_message, UnifiedMessage,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub(crate) fn open_code(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let mut seen = HashSet::new();
    let mut messages = Vec::new();

    for db_path in &context.scan.opencode_dbs {
        messages.extend(
            sessions::opencode::parse_opencode_sqlite(db_path)
                .into_iter()
                .filter(|message| {
                    message
                        .dedup_key
                        .as_ref()
                        .is_none_or(|key| key.is_empty() || seen.insert(key.clone()))
                }),
        );
    }

    let json_messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .filter_map(|path| sessions::opencode::parse_opencode_file(path))
        .collect::<Vec<_>>();
    messages.extend(json_messages.into_iter().filter(|message| {
        message
            .dedup_key
            .as_ref()
            .is_none_or(|key| key.is_empty() || seen.insert(key.clone()))
    }));
    context.append(spec, messages);
}

pub(crate) fn claude(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let claude_home = PathBuf::from(context.home_dir);
    let raw = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map_init(HashMap::new, |parent_cache, path| {
            sessions::claudecode::parse_claude_file_with_cache_and_home(
                path,
                parent_cache,
                Some(&claude_home),
            )
        })
        .flatten()
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let messages = raw
        .into_iter()
        .filter(|message| {
            message
                .dedup_key
                .as_ref()
                .is_none_or(|key| key.is_empty() || seen.insert(key.clone()))
        })
        .collect();
    context.append(spec, messages);
}

pub(crate) fn codex(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Codex local parser");
    let raw: Vec<UnifiedMessage> = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| {
            let is_headless = is_headless_path(path, &context.headless_roots);
            parser(path)
                .into_iter()
                .map(|mut message| {
                    apply_headless_agent(&mut message, is_headless);
                    message
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut seen = HashSet::new();
    let messages = raw
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut seen, message))
        .collect();
    context.append(spec, messages);
}
