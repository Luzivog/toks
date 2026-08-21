use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod tracker_wire;

/// Parser-owned identity for one durable usage fact.
///
/// Unlike `dedup_key`, this identity never includes mutable attribution or
/// accounting fields such as model, tokens, or cost.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableIdentityScheme {
    ClaudeProviderResponse,
    CodexSessionTimestampOccurrence,
    CodexSessionRecordSequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStrength {
    Strong,
    SessionStable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableIdentity {
    pub scheme: DurableIdentityScheme,
    /// Identity algorithm version. This participates in the archive key: never
    /// bump it without an explicit, tested alias/migration from the prior
    /// scheme. Automatically aliasing equal-strength versions is unsafe.
    pub version: u32,
    pub value: String,
    pub strength: IdentityStrength,
}

impl DurableIdentity {
    pub(crate) fn claude_provider_response(value: String) -> Self {
        Self {
            scheme: DurableIdentityScheme::ClaudeProviderResponse,
            version: 1,
            value,
            strength: IdentityStrength::Strong,
        }
    }
}

/// Best-effort equivalence supplied by a parser's existing scan-time dedup.
///
/// An accounting alias may use mutable source facts and must never replace a
/// durable identity or become a revision key. It only links observations that
/// the current scan already treats as interchangeable representatives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingAliasScheme {
    CodexForkReplayDedup,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountingAlias {
    pub scheme: AccountingAliasScheme,
    /// Alias algorithm version. Archive migrations must explicitly account for
    /// any change; aliases from different versions are never equal implicitly.
    pub version: u32,
    /// Opaque digest. Raw scan dedup keys are intentionally not persisted here.
    pub value: String,
}

pub(crate) fn codex_fork_replay_alias(dedup_key: &str) -> AccountingAlias {
    let digest = Sha256::digest(dedup_key.as_bytes());
    AccountingAlias {
        scheme: AccountingAliasScheme::CodexForkReplayDedup,
        version: 1,
        value: format!("{digest:x}"),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TimestampOccurrence(Box<str>, u64);

type TimestampOccurrences = BTreeMap<Box<str>, Vec<TimestampOccurrence>>;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub(crate) struct CodexIdentityTracker {
    #[serde(default)]
    token_count_sequence: u64,
    /// Each lineage is stored once, and its timestamps are kept sorted. The
    /// previous wire format repeated the encoded lineage in every map key.
    #[serde(default)]
    timestamp_occurrences: TimestampOccurrences,
}

impl CodexIdentityTracker {
    pub(crate) fn next(
        &mut self,
        parent_session_id: Option<&str>,
        logical_session_id: &str,
        raw_timestamp: Option<&str>,
    ) -> DurableIdentity {
        let sequence = self.token_count_sequence;
        self.token_count_sequence = sequence.saturating_add(1);
        let lineage = parent_session_id.map_or_else(
            || encode_parts(&[logical_session_id]),
            |parent| encode_parts(&[parent, logical_session_id]),
        );
        if let Some(timestamp) = raw_timestamp.filter(|value| !value.is_empty()) {
            let current = self.next_timestamp_occurrence(&lineage, timestamp);
            return DurableIdentity {
                scheme: DurableIdentityScheme::CodexSessionTimestampOccurrence,
                version: 1,
                value: encode_parts(&[&lineage, timestamp, &current.to_string()]),
                strength: IdentityStrength::SessionStable,
            };
        }
        DurableIdentity {
            scheme: DurableIdentityScheme::CodexSessionRecordSequence,
            version: 1,
            value: encode_parts(&[&lineage, &sequence.to_string()]),
            strength: IdentityStrength::SessionStable,
        }
    }

    fn next_timestamp_occurrence(&mut self, lineage: &str, timestamp: &str) -> u64 {
        let entries = if let Some(entries) = self.timestamp_occurrences.get_mut(lineage) {
            entries
        } else {
            self.timestamp_occurrences
                .entry(lineage.into())
                .or_default()
        };
        match entries.binary_search_by(|entry| entry.0.as_ref().cmp(timestamp)) {
            Ok(index) => {
                let current = entries[index].1;
                entries[index].1 = current.saturating_add(1);
                current
            }
            Err(index) => {
                entries.insert(index, TimestampOccurrence(timestamp.into(), 1));
                0
            }
        }
    }
}

fn encode_parts(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| format!("{}:{part}", part.len()))
        .collect::<Vec<_>>()
        .join("|")
}
