use super::LocalContext;
use crate::ingest::registry::ClientParserDef;
use crate::{bucket_tz, dedupe_latest_trae_messages, sessions};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub(crate) fn kimi(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| {
            if sessions::kimi::is_kimi_code_path(path) {
                sessions::kimi::parse_kimi_code_file(path)
            } else {
                sessions::kimi::parse_kimi_file(path)
            }
        })
        .collect();
    context.append(spec, messages);
}

pub(crate) fn crush(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let timezone = bucket_tz::BucketTimezone::from_scanner_settings(context.scanner_settings);
    let messages = context
        .scan
        .crush_dbs
        .par_iter()
        .flat_map(|source| {
            sessions::crush::parse_crush_sqlite_in(&source.db_path, &timezone)
                .into_iter()
                .map(|mut message| {
                    message.set_workspace(
                        source.workspace_key.clone(),
                        source.workspace_label.clone(),
                    );
                    message
                })
                .collect::<Vec<_>>()
        })
        .collect();
    context.append(spec, messages);
}

pub(crate) fn trae(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| sessions::trae::parse_trae_file("trae", path))
        .collect();
    context.append(spec, dedupe_latest_trae_messages(messages));
}

pub(crate) fn grok(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("Grok file parser");
    let messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| parser(path))
        .collect();
    context.append(spec, sessions::grok::prefer_unified_log_messages(messages));
}
