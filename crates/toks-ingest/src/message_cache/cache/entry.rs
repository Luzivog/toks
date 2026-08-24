use super::{CacheIdentity, CachedSourceEntry};
use crate::clients::ClientId;
use crate::message_cache::{CachedPath, CodexIncrementalCache, SourceFingerprint};
use crate::UnifiedMessage;
use std::collections::HashSet;
use std::path::Path;

impl CachedSourceEntry {
    pub(crate) fn new(
        identity: CacheIdentity,
        path: &Path,
        fingerprint: SourceFingerprint,
        messages: Vec<UnifiedMessage>,
        fallback_timestamp_indices: Vec<usize>,
        codex_incremental: Option<CodexIncrementalCache>,
    ) -> Self {
        Self {
            parser_namespace: identity.namespace.to_string(),
            parser_version: identity.parser_version,
            path: CachedPath::from_path(path),
            fingerprint,
            messages,
            fallback_timestamp_indices,
            codex_incremental,
            prime_accounting: None,
        }
    }

    pub(crate) fn with_prime_accounting(
        mut self,
        accounting: crate::sessions::prime_agent::PrimeFileAccounting,
    ) -> Self {
        self.prime_accounting = Some(accounting);
        self
    }

    pub(super) fn identity_is_current(&self) -> bool {
        CacheIdentity::current_for_namespace(&self.parser_namespace)
            .is_some_and(|identity| identity.parser_version == self.parser_version)
    }

    /// Carry forward keyed messages an entry already on disk holds for this
    /// same path and this one does not.
    ///
    /// Two processes can scan at once — a running TUI and a `tokscope submit`,
    /// say. Each loads the entry, parses, and saves back, and the last writer
    /// replaces the other's entry wholesale. For most namespaces that is
    /// harmless: the loser's messages come from the same bytes and reappear on
    /// the next scan. For a namespace that retains history it is not, because
    /// the messages the loser observed are gone from the live file too, so
    /// nothing will ever put them back.
    ///
    /// Same filter as the parse-time merge: a key that is only unique within
    /// one file must not outlive the bytes that produced it.
    pub(super) fn absorb_retained_history(&mut self, stored: &CachedSourceEntry) {
        let Some(key_is_globally_stable) = retained_history_key_filter(&self.parser_namespace)
        else {
            return;
        };
        // A stored entry from a different parser version describes a layout
        // this one does not agree with; let the wholesale replace stand.
        if stored.parser_namespace != self.parser_namespace
            || stored.parser_version != self.parser_version
        {
            return;
        }

        let mut seen: HashSet<String> = self
            .messages
            .iter()
            .filter_map(|message| message.dedup_key.clone())
            .collect();
        for message in &stored.messages {
            let Some(key) = message.dedup_key.as_ref() else {
                continue;
            };
            if !key_is_globally_stable(key) {
                continue;
            }
            if seen.insert(key.clone()) {
                self.messages.push(message.clone());
            }
        }
    }
}

/// The dedup-key filter for namespaces whose entries carry history the live
/// file may no longer contain, or `None` for namespaces that do not retain
/// history.
///
/// Mirrors the `HistoryRetention` choice each lane makes in `lib.rs`. It has to
/// exist here as well because the save merge is the other place a retained
/// message can be dropped, and it must honor the same contract.
fn retained_history_key_filter(namespace: &str) -> Option<fn(&str) -> bool> {
    (namespace == ClientId::Claude.as_str())
        .then_some(crate::sessions::claudecode::dedup_key_is_globally_stable)
}
