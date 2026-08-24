use super::super::cache::{apply_pricing_to_messages, load_or_parse_prime_source};
use super::super::registry::ClientParserDef;
use super::PricedContext;
use crate::sessions;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub(crate) fn prime(context: &mut PricedContext<'_>, spec: &'static ClientParserDef) {
    let outcomes = context
        .scan
        .get(spec.scan_bucket)
        .par_iter()
        .map(|path| load_or_parse_prime_source(path, &context.source_cache, context.pricing))
        .collect::<Vec<_>>();
    let mut messages = Vec::new();
    let mut accounting = Vec::new();
    for (outcome, file_accounting) in outcomes {
        messages.extend(outcome.messages);
        accounting.push(file_accounting);
        if let Some(entry) = outcome.cache_entry {
            context.source_cache.insert(entry);
        }
    }
    let mut reconciled =
        sessions::prime_agent::reconcile_prime_agent_messages(messages, &accounting);
    apply_pricing_to_messages(&mut reconciled, context.pricing);
    context.messages.extend(reconciled);
}
