mod cache;
mod helpers;
pub(crate) mod local;
pub(crate) mod priced;
mod registry;

pub(crate) use helpers::{
    apply_headless_agent, dedupe_latest_trae_messages, is_headless_path, merge_workbuddy_messages,
    parse_hermes_sqlite_with_pricing, partition_workbuddy_paths, rebucket_days,
};
pub use local::parse_local_clients;
pub(crate) use priced::{
    parse_all_messages_with_pricing_with_cache_policy,
    parse_all_messages_with_pricing_with_env_strategy,
};

use crate::{
    filter_unified_messages, get_home_dir_string, load_pricing_for_local_parse, pricing, sessions,
    ClientId, LocalParseOptions, UnifiedMessage,
};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceCachePolicy {
    Persistent,
    InMemory,
}

pub(crate) fn retain_for_requested_clients(
    client: &str,
    model_id: &str,
    provider_id: &str,
    requested: &HashSet<&str>,
) -> bool {
    requested.contains(client)
        || (requested.contains("claude") && client.starts_with("cc-mirror/"))
        || (requested.contains("gjc") && client.eq_ignore_ascii_case("9router"))
        || (requested.contains("synthetic")
            && sessions::synthetic::matches_synthetic_filter(client, model_id, provider_id))
}

fn resolve_local_parse_request(
    options: &LocalParseOptions,
) -> Result<(String, Vec<String>), String> {
    let home_dir = get_home_dir_string(&options.home_dir)?;
    let clients = options.clients.clone().unwrap_or_else(|| {
        let mut clients = ClientId::iter()
            .filter(ClientId::parse_local)
            .map(|client| client.as_str().to_string())
            .collect::<Vec<_>>();
        clients.push("synthetic".to_string());
        clients
    });
    Ok((home_dir, clients))
}

fn parse_local_unified_messages_resolved(
    options: LocalParseOptions,
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
    cache_policy: SourceCachePolicy,
) -> Vec<UnifiedMessage> {
    let messages = parse_all_messages_with_pricing_with_cache_policy(
        home_dir,
        clients,
        pricing,
        options.use_env_roots,
        &options.scanner_settings,
        cache_policy,
    );
    filter_unified_messages(messages, &options)
}

#[doc(hidden)]
pub async fn parse_local_unified_messages_with_pricing(
    options: LocalParseOptions,
    pricing: Option<&pricing::PricingService>,
) -> Result<Vec<UnifiedMessage>, String> {
    let (home_dir, clients) = resolve_local_parse_request(&options)?;
    Ok(parse_local_unified_messages_resolved(
        options,
        &home_dir,
        &clients,
        pricing,
        SourceCachePolicy::Persistent,
    ))
}

#[doc(hidden)]
pub async fn parse_local_unified_messages_with_pricing_uncached(
    options: LocalParseOptions,
    pricing: Option<&pricing::PricingService>,
) -> Result<Vec<UnifiedMessage>, String> {
    let (home_dir, clients) = resolve_local_parse_request(&options)?;
    Ok(parse_local_unified_messages_resolved(
        options,
        &home_dir,
        &clients,
        pricing,
        SourceCachePolicy::InMemory,
    ))
}

pub async fn parse_local_unified_messages(
    options: LocalParseOptions,
) -> Result<Vec<UnifiedMessage>, String> {
    let (home_dir, clients) = resolve_local_parse_request(&options)?;
    let pricing = load_pricing_for_local_parse().await;
    Ok(parse_local_unified_messages_resolved(
        options,
        &home_dir,
        &clients,
        pricing.as_deref(),
        SourceCachePolicy::Persistent,
    ))
}
