use super::LocalContext;
use crate::ingest::registry::{ClientParserDef, MergePolicy};
use crate::should_keep_deduped_message;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::HashSet;

pub(crate) fn files(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    if !context.enabled(spec) {
        return;
    }
    let parser = spec.parser.expect("file parser registry entry");
    let mut messages = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .flat_map(|path| parser(path))
        .collect::<Vec<_>>();

    match spec.merge {
        MergePolicy::Append => {}
        MergePolicy::Dedup => {
            let mut seen = HashSet::new();
            messages.retain(|message| should_keep_deduped_message(&mut seen, message));
        }
        _ => unreachable!("special merge policy reached the common local parser"),
    }
    context.append(spec, messages);
}
