use super::super::cache::CachedParseOutcome;
use super::super::registry::{ClientParserDef, MergePolicy, PricingPolicy, RequestPolicy};
use crate::{
    apply_pricing_if_available, message_cache, pricing, ClientId, ScanResult, ScannerSettings,
    UnifiedMessage,
};
use std::collections::HashSet;
use std::path::PathBuf;

pub(crate) struct PricedContext<'a> {
    pub(super) scan: ScanResult,
    pub(super) source_cache: message_cache::SourceMessageCache,
    pub(super) pricing: Option<&'a pricing::PricingService>,
    pub(super) home_dir: &'a str,
    pub(super) headless_roots: Vec<PathBuf>,
    pub(super) clients: &'a [String],
    pub(super) scanner_settings: &'a ScannerSettings,
    pub(super) include_all: bool,
    pub(super) include_synthetic: bool,
    pub(super) messages: Vec<UnifiedMessage>,
    pub(super) devin_cli_session_ids: HashSet<String>,
}

impl<'a> PricedContext<'a> {
    pub(super) fn new(
        scan: ScanResult,
        source_cache: message_cache::SourceMessageCache,
        pricing: Option<&'a pricing::PricingService>,
        home_dir: &'a str,
        headless_roots: Vec<PathBuf>,
        clients: &'a [String],
        scanner_settings: &'a ScannerSettings,
    ) -> Self {
        let include_all = clients.is_empty();
        Self {
            scan,
            source_cache,
            pricing,
            home_dir,
            headless_roots,
            clients,
            scanner_settings,
            include_all,
            include_synthetic: include_all || clients.iter().any(|client| client == "synthetic"),
            messages: Vec::new(),
            devin_cli_session_ids: HashSet::new(),
        }
    }

    pub(super) fn enabled(&self, spec: &ClientParserDef) -> bool {
        match spec.request {
            RequestPolicy::ScannerSelected => true,
            RequestPolicy::ExplicitOrAll => {
                self.include_all
                    || self
                        .clients
                        .iter()
                        .any(|client| client == spec.client.as_str())
            }
            RequestPolicy::ExplicitOrSynthetic => {
                self.include_synthetic
                    || self
                        .clients
                        .iter()
                        .any(|client| client == spec.client.as_str())
            }
        }
    }

    pub(super) fn identity(&self, spec: &ClientParserDef) -> message_cache::CacheIdentity {
        message_cache::CacheIdentity::for_client(
            spec.cache_identity.expect("cached registry entry"),
        )
    }

    pub(super) fn price(&self, message: &mut UnifiedMessage, policy: PricingPolicy) {
        match policy {
            PricingPolicy::Reprice => apply_pricing_if_available(message, self.pricing),
            PricingPolicy::FillNonPositive if message.cost <= 0.0 => {
                apply_pricing_if_available(message, self.pricing);
            }
            PricingPolicy::FillNonAuthoritative if !message.has_authoritative_cost() => {
                apply_pricing_if_available(message, self.pricing);
            }
            PricingPolicy::Preserve
            | PricingPolicy::FillNonPositive
            | PricingPolicy::FillNonAuthoritative => {}
        }
    }

    pub(super) fn drain(&mut self, spec: &ClientParserDef, outcomes: Vec<CachedParseOutcome>) {
        let mut seen = HashSet::new();
        for outcome in outcomes {
            match spec.merge {
                MergePolicy::Append => self.messages.extend(outcome.messages),
                MergePolicy::Dedup => {
                    self.messages
                        .extend(outcome.messages.into_iter().filter(|message| {
                            message
                                .dedup_key
                                .as_ref()
                                .is_none_or(|key| seen.insert(key.clone()))
                        }))
                }
                _ => unreachable!("special merge policy reached the common cache drain"),
            }
            if let Some(entry) = outcome.cache_entry {
                self.source_cache.insert(entry);
            }
        }
    }

    pub(super) fn drain_append(&mut self, outcomes: Vec<CachedParseOutcome>) {
        for outcome in outcomes {
            self.messages.extend(outcome.messages);
            if let Some(entry) = outcome.cache_entry {
                self.source_cache.insert(entry);
            }
        }
    }

    pub(super) fn remove_cache(&mut self, client: ClientId, path: &std::path::Path) {
        self.source_cache
            .remove(message_cache::CacheIdentity::for_client(client), path);
    }
}
