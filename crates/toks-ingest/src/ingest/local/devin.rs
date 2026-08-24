use super::LocalContext;
use crate::ingest::registry::ClientParserDef;
use crate::{sessions, should_keep_deduped_message};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn devin_cli(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let parser = spec.parser.expect("Devin CLI SQLite parser");
    let mut seen = HashSet::new();
    let messages = context
        .scan
        .devin_dbs
        .iter()
        .flat_map(|path| parser(path))
        .filter(|message| should_keep_deduped_message(&mut seen, message))
        .collect::<Vec<_>>();
    context
        .devin_cli_session_ids
        .extend(messages.iter().map(|message| message.session_id.clone()));
    context.append(spec, messages);
}

pub(crate) fn devin_desktop(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let lookup = sessions::devin::load_devin_desktop_session_lookup(&context.scan.devin_dbs);
    let raw = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| sessions::devin::parse_devin_desktop_ndjson_with_lookup(path, &lookup))
        .collect::<Vec<_>>();
    let raw_count = context.count(spec, &raw);
    let messages = raw
        .into_iter()
        .filter(|message| !context.devin_cli_session_ids.contains(&message.session_id))
        .collect();
    context.set_count(spec, raw_count);
    context.append_without_count(messages);
}
