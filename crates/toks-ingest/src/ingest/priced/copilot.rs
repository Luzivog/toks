use super::super::cache::load_or_parse_source;
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::sessions;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn copilot(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Copilot file parser");
    let identity = context.identity(spec);
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
                parser,
            )
        })
        .collect::<Vec<_>>();
    context.drain_append(outcomes);

    if let Some(path) = &context.scan.copilot_desktop_db {
        let otel_sessions: HashSet<String> = context
            .messages
            .iter()
            .filter(|message| message.client == "copilot")
            .map(|message| message.session_id.clone())
            .collect();
        let desktop = sessions::copilot_desktop::parse_copilot_desktop_db(path)
            .into_iter()
            .filter(|message| !otel_sessions.contains(&message.session_id))
            .map(|mut message| {
                context.price(&mut message, spec.pricing);
                message
            })
            .collect::<Vec<_>>();
        context.messages.extend(desktop);
    }

    let existing_keys: HashSet<String> = context
        .messages
        .iter()
        .filter(|message| message.client == "copilot")
        .filter_map(|message| message.dedup_key.clone())
        .collect();
    let existing_session_timestamps: HashSet<(String, i64)> = context
        .messages
        .iter()
        .filter(|message| message.client == "copilot")
        .map(|message| (message.session_id.clone(), message.timestamp))
        .collect();
    let vscode = sessions::copilot_vscode::parse_copilot_vscode_sessions(
        &context.scan.copilot_vscode_sessions,
    )
    .into_iter()
    .filter(|message| {
        let key_unique = message
            .dedup_key
            .as_deref()
            .is_none_or(|key| !existing_keys.contains(key));
        let timestamp_unique =
            !existing_session_timestamps.contains(&(message.session_id.clone(), message.timestamp));
        key_unique && timestamp_unique
    })
    .map(|mut message| {
        context.price(&mut message, spec.pricing);
        message
    })
    .collect::<Vec<_>>();
    context.messages.extend(vscode);
}
