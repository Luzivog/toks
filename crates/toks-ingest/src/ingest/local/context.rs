use super::super::registry::{ClientParserDef, CountPolicy, RequestPolicy};
use crate::{scanner, unified_to_parsed, ClientCounts, ParsedMessage, ScanResult, UnifiedMessage};
use std::collections::HashSet;
use std::path::PathBuf;

pub(crate) struct LocalContext<'a> {
    pub(super) scan: ScanResult,
    pub(super) home_dir: &'a str,
    pub(super) headless_roots: Vec<PathBuf>,
    pub(super) clients: &'a [String],
    pub(super) scanner_settings: &'a scanner::ScannerSettings,
    pub(super) include_all: bool,
    pub(super) include_synthetic: bool,
    pub(super) messages: Vec<ParsedMessage>,
    pub(super) counts: ClientCounts,
    pub(super) devin_cli_session_ids: HashSet<String>,
}

impl<'a> LocalContext<'a> {
    pub(super) fn new(
        scan: ScanResult,
        home_dir: &'a str,
        headless_roots: Vec<PathBuf>,
        clients: &'a [String],
        scanner_settings: &'a scanner::ScannerSettings,
    ) -> Self {
        let include_all = clients.is_empty();
        Self {
            scan,
            home_dir,
            headless_roots,
            clients,
            scanner_settings,
            include_all,
            include_synthetic: include_all || clients.iter().any(|client| client == "synthetic"),
            messages: Vec::new(),
            counts: ClientCounts::new(),
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

    pub(super) fn count(&self, spec: &ClientParserDef, messages: &[UnifiedMessage]) -> i32 {
        match spec.count {
            CountPolicy::None => 0,
            CountPolicy::Rows => messages.len() as i32,
            CountPolicy::Messages | CountPolicy::AdditiveMessages | CountPolicy::RawBeforeMerge => {
                messages
                    .iter()
                    .map(|message| message.message_count.max(0))
                    .sum()
            }
            CountPolicy::SaturatingRawMessages => messages.iter().fold(0_i32, |count, message| {
                count.saturating_add(message.message_count)
            }),
        }
    }

    pub(super) fn set_count(&mut self, spec: &ClientParserDef, count: i32) {
        if let Some(bucket) = spec.count_bucket {
            self.counts.set(bucket, count);
        }
    }

    pub(super) fn add_count(&mut self, spec: &ClientParserDef, count: i32) {
        if let Some(bucket) = spec.count_bucket {
            self.counts.add(bucket, count);
        }
    }

    pub(super) fn append(&mut self, spec: &ClientParserDef, messages: Vec<UnifiedMessage>) {
        let count = self.count(spec, &messages);
        self.set_count(spec, count);
        self.messages.extend(messages.iter().map(unified_to_parsed));
    }

    pub(super) fn append_without_count(&mut self, messages: Vec<UnifiedMessage>) {
        self.messages.extend(messages.iter().map(unified_to_parsed));
    }
}
