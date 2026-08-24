mod codex;
mod codex_entry;
mod history;
mod loaders;
mod prime;

pub(crate) use codex::load_or_parse_codex_source;
pub(crate) use history::{load_or_parse_source_with_fingerprint_and_policy, HistoryRetention};
pub(crate) use loaders::{
    load_or_parse_source, load_or_parse_source_with_fingerprint,
    load_or_parse_source_with_fingerprint_context,
    load_or_parse_source_with_fingerprint_retaining_history, load_or_parse_sqlite_source,
};
pub(crate) use prime::load_or_parse_prime_source;

use crate::{apply_pricing_if_available, message_cache, pricing, UnifiedMessage};

#[derive(Debug)]
pub(crate) struct CachedParseOutcome {
    pub(crate) messages: Vec<UnifiedMessage>,
    pub(crate) cache_entry: Option<message_cache::CachedSourceEntry>,
    pub(crate) invalidate_cache: bool,
}

pub(crate) fn apply_pricing_to_messages(
    messages: &mut [UnifiedMessage],
    pricing: Option<&pricing::PricingService>,
) {
    for message in messages {
        message.refresh_derived_fields();
        apply_pricing_if_available(message, pricing);
    }
}

pub(crate) fn cached_messages(
    cached: &message_cache::CachedSourceEntry,
    pricing: Option<&pricing::PricingService>,
) -> Vec<UnifiedMessage> {
    let mut messages = cached.messages.clone();
    apply_pricing_to_messages(&mut messages, pricing);
    messages
}
