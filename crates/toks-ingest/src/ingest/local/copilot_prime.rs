use super::LocalContext;
use crate::ingest::registry::ClientParserDef;
use crate::sessions;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn copilot(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Copilot file parser");
    let mut messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| parser(path))
        .collect::<Vec<_>>();

    if let Some(db_path) = &context.scan.copilot_desktop_db {
        let otel_sessions: HashSet<String> = messages
            .iter()
            .map(|message| message.session_id.clone())
            .collect();
        messages.extend(
            sessions::copilot_desktop::parse_copilot_desktop_db(db_path)
                .into_iter()
                .filter(|message| !otel_sessions.contains(&message.session_id)),
        );
    }

    let existing_keys: HashSet<String> = messages
        .iter()
        .filter_map(|message| message.dedup_key.clone())
        .collect();
    let existing_session_timestamps: HashSet<(String, i64)> = messages
        .iter()
        .map(|message| (message.session_id.clone(), message.timestamp))
        .collect();
    messages.extend(
        sessions::copilot_vscode::parse_copilot_vscode_sessions(
            &context.scan.copilot_vscode_sessions,
        )
        .into_iter()
        .filter(|message| {
            let key_unique = message
                .dedup_key
                .as_deref()
                .is_none_or(|key| !existing_keys.contains(key));
            let timestamp_unique = !existing_session_timestamps
                .contains(&(message.session_id.clone(), message.timestamp));
            key_unique && timestamp_unique
        }),
    );
    context.append(spec, messages);
}

pub(crate) fn prime(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let files = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| sessions::prime_agent::parse_prime_agent_file_with_accounting(path))
        .collect::<Vec<_>>();
    let mut messages = Vec::new();
    let mut accounting = Vec::new();
    for (file_messages, file_accounting) in files {
        messages.extend(file_messages);
        accounting.push(file_accounting);
    }
    context.append(
        spec,
        sessions::prime_agent::reconcile_prime_agent_messages(messages, &accounting),
    );
}
