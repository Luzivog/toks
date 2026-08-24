use super::LocalContext;
use crate::ingest::registry::ClientParserDef;
use crate::{sessions, should_keep_deduped_message};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn zcode(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let mut messages = context
        .scan
        .zcode_db
        .as_ref()
        .map(|path| sessions::zcode::parse_zcode_sqlite(path))
        .unwrap_or_default();
    messages.extend(
        context
            .scan
            .get(spec.scan_bucket)
            .par_iter()
            .flat_map(|path| sessions::zcode::parse_zcode_file(path))
            .collect::<Vec<_>>(),
    );
    context.append(spec, messages);
}

pub(crate) fn kilo(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let Some(path) = &context.scan.kilo_db else {
        return;
    };
    let parser = spec.parser.expect("Kilo SQLite parser");
    context.append(spec, parser(path));
}

pub(crate) fn hermes(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let paths = context.scan.hermes_db_paths();
    if paths.is_empty() {
        return;
    }
    let parser = spec.parser.expect("Hermes SQLite parser");
    let mut seen = HashSet::new();
    let messages = paths
        .iter()
        .flat_map(|path| parser(path))
        .filter(|message| should_keep_deduped_message(&mut seen, message))
        .collect();
    context.append(spec, messages);
}

pub(crate) fn goose(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let Some(path) = &context.scan.goose_db else {
        return;
    };
    let parser = spec.parser.expect("Goose SQLite parser");
    context.append(spec, parser(path));
}

pub(crate) fn zed(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let paths = context.scan.zed_db_paths();
    if paths.is_empty() {
        return;
    }
    let parser = spec.parser.expect("Zed SQLite parser");
    context.append(spec, paths.iter().flat_map(|path| parser(path)).collect());
}

pub(crate) fn kiro(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Kiro file parser");
    let file_messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| parser(path))
        .collect();
    let file_messages = sessions::kiro::suppress_snapshots_covered_by_executions(file_messages);
    let file_count = context.count(spec, &file_messages);
    context.set_count(spec, file_count);
    context.append_without_count(file_messages);

    if let Some(path) = &context.scan.kiro_db {
        let database_messages = sessions::kiro::parse_kiro_sqlite(path);
        let database_count = context.count(spec, &database_messages);
        context.add_count(spec, database_count);
        context.append_without_count(database_messages);
    }
}
