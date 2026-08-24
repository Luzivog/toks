use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::{bucket_tz, dedupe_latest_trae_messages, sessions};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub(crate) fn crush(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let timezone = bucket_tz::BucketTimezone::from_scanner_settings(context.scanner_settings);
    let mut messages = context
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
        .collect::<Vec<_>>();
    for message in &mut messages {
        context.price(message, spec.pricing);
    }
    context.messages.extend(messages);
}

pub(crate) fn trae(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| sessions::trae::parse_trae_file("trae", path))
        .collect();
    context
        .messages
        .extend(dedupe_latest_trae_messages(messages));
}
