mod claude_codex;
mod context;
mod copilot;
mod databases;
mod devin;
mod files;
mod open_code;
mod others;
mod prime;
mod variable_files;
mod workbuddy;

pub(crate) use claude_codex::{claude, codex};
pub(crate) use context::PricedContext;
pub(crate) use copilot::copilot;
pub(crate) use databases::{goose, hermes, kilo, kiro, zcode, zed};
pub(crate) use devin::{devin_cli, devin_desktop};
pub(crate) use files::{cached_files, uncached_files};
pub(crate) use open_code::{micode, open_code};
pub(crate) use others::{crush, trae};
pub(crate) use prime::prime;
pub(crate) use variable_files::{gemini, grok, kimi};
pub(crate) use workbuddy::workbuddy;

use super::cache::load_or_parse_sqlite_source;
use super::registry::{spec, PRICED_ORDER};
use super::SourceCachePolicy;
use crate::{
    message_cache, pricing, rebucket_days, retain_for_requested_clients, scanner, sessions,
    UnifiedMessage,
};
use std::collections::HashSet;

pub(crate) fn parse_all_messages_with_pricing_with_env_strategy(
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
    use_env_roots: bool,
    scanner_settings: &scanner::ScannerSettings,
) -> Vec<UnifiedMessage> {
    parse_all_messages_with_pricing_with_cache_policy(
        home_dir,
        clients,
        pricing,
        use_env_roots,
        scanner_settings,
        SourceCachePolicy::Persistent,
    )
}

pub(crate) fn parse_all_messages_with_pricing_with_cache_policy(
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
    use_env_roots: bool,
    scanner_settings: &scanner::ScannerSettings,
    cache_policy: SourceCachePolicy,
) -> Vec<UnifiedMessage> {
    let scan = scanner::scan_all_clients_with_scanner_settings(
        home_dir,
        clients,
        use_env_roots,
        scanner_settings,
    );
    let headless_roots = scanner::headless_roots_with_env_strategy(home_dir, use_env_roots);
    let source_cache = match cache_policy {
        SourceCachePolicy::Persistent => {
            let mut cache = message_cache::SourceMessageCache::load_for_clients(clients);
            cache.prune_missing_files();
            cache
        }
        SourceCachePolicy::InMemory => message_cache::SourceMessageCache::default(),
    };
    let mut context = PricedContext::new(
        scan,
        source_cache,
        pricing,
        home_dir,
        headless_roots,
        clients,
        scanner_settings,
    );

    for client in PRICED_ORDER {
        let definition = spec(client);
        definition.assert_coherent();
        (definition.priced)(&mut context, definition);
    }
    parse_synthetic(&mut context);

    if !context.include_all {
        let requested: HashSet<&str> = clients.iter().map(String::as_str).collect();
        context.messages.retain(|message| {
            retain_for_requested_clients(
                &message.client,
                &message.model_id,
                &message.provider_id,
                &requested,
            )
        });
    }
    if context.include_synthetic {
        for message in &mut context.messages {
            sessions::synthetic::normalize_synthetic_gateway_fields(
                &mut message.model_id,
                &mut message.provider_id,
            );
        }
    }
    if cache_policy == SourceCachePolicy::Persistent {
        context.source_cache.save_if_dirty();
    }
    rebucket_days(&mut context.messages, scanner_settings);
    context.messages
}

fn parse_synthetic(context: &mut PricedContext<'_>) {
    if !context.include_synthetic {
        return;
    }
    let Some(path) = context.scan.synthetic_db.clone() else {
        return;
    };
    let outcome = load_or_parse_sqlite_source(
        message_cache::CacheIdentity::synthetic(),
        &path,
        &context.source_cache,
        context.pricing,
        sessions::synthetic::parse_octofriend_sqlite,
    );
    context.messages.extend(outcome.messages);
    if let Some(entry) = outcome.cache_entry {
        context.source_cache.insert(entry);
    }
}
