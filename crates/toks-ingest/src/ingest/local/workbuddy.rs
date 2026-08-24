use super::LocalContext;
use crate::ingest::registry::ClientParserDef;
use crate::{merge_workbuddy_messages, partition_workbuddy_paths};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub(crate) fn workbuddy(context: &mut LocalContext<'_>, spec: &'static ClientParserDef) {
    let parser = spec.parser.expect("WorkBuddy parser");
    let (detailed_paths, fallback_paths) =
        partition_workbuddy_paths(context.scan.get(spec.scan_bucket));
    let detailed = detailed_paths
        .par_iter()
        .flat_map(|path| parser(path))
        .collect();
    let fallback = fallback_paths
        .par_iter()
        .flat_map(|path| parser(path))
        .collect();
    context.append(spec, merge_workbuddy_messages(detailed, fallback));
}
