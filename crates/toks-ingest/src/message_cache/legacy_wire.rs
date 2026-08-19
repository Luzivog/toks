use super::legacy_codex::LegacyCodexIncrementalCache;
use super::{CachedPath, CachedSourceEntry, SourceFingerprint};
use crate::{CostSource, UnifiedMessage};
use bincode::Options;
use serde::{Deserialize, Serialize};

pub(super) const FORMAT_V5: u32 = 5;
pub(super) const FORMAT_V4: u32 = 4;

/// Exact pre-v6 message layout. Bincode cannot apply a serde default to a new
/// trailing struct field, so old cache payloads decode through this wire type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LegacyUnifiedMessageV5 {
    client: String,
    model_id: String,
    provider_id: String,
    session_id: String,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
    timestamp: i64,
    date: String,
    tokens: crate::TokenBreakdown,
    cost: f64,
    cost_source: CostSource,
    duration_ms: Option<i64>,
    message_count: i32,
    agent: Option<String>,
    dedup_key: Option<String>,
    session_title: Option<String>,
    is_turn_start: bool,
    model_attribution_conflicted: bool,
}

impl From<LegacyUnifiedMessageV5> for UnifiedMessage {
    fn from(message: LegacyUnifiedMessageV5) -> Self {
        Self {
            client: message.client,
            model_id: message.model_id,
            provider_id: message.provider_id,
            session_id: message.session_id,
            workspace_key: message.workspace_key,
            workspace_label: message.workspace_label,
            timestamp: message.timestamp,
            date: message.date,
            tokens: message.tokens,
            cost: message.cost,
            cost_source: message.cost_source,
            duration_ms: message.duration_ms,
            message_count: message.message_count,
            agent: message.agent,
            dedup_key: message.dedup_key,
            durable_identity: None,
            accounting_aliases: Vec::new(),
            session_title: message.session_title,
            is_turn_start: message.is_turn_start,
            model_attribution_conflicted: message.model_attribution_conflicted,
        }
    }
}

#[cfg(test)]
impl From<UnifiedMessage> for LegacyUnifiedMessageV5 {
    fn from(message: UnifiedMessage) -> Self {
        Self {
            client: message.client,
            model_id: message.model_id,
            provider_id: message.provider_id,
            session_id: message.session_id,
            workspace_key: message.workspace_key,
            workspace_label: message.workspace_label,
            timestamp: message.timestamp,
            date: message.date,
            tokens: message.tokens,
            cost: message.cost,
            cost_source: message.cost_source,
            duration_ms: message.duration_ms,
            message_count: message.message_count,
            agent: message.agent,
            dedup_key: message.dedup_key,
            session_title: message.session_title,
            is_turn_start: message.is_turn_start,
            model_attribution_conflicted: message.model_attribution_conflicted,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LegacyCachedSourceEntryV5 {
    pub parser_namespace: String,
    pub parser_version: u32,
    pub path: CachedPath,
    pub fingerprint: SourceFingerprint,
    pub messages: Vec<LegacyUnifiedMessageV5>,
    pub fallback_timestamp_indices: Vec<usize>,
    pub codex_incremental: Option<LegacyCodexIncrementalCache>,
    pub prime_accounting: Option<crate::sessions::prime_agent::PrimeFileAccounting>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LegacyCachedSourceEntryV4 {
    pub parser_namespace: String,
    pub parser_version: u32,
    pub path: CachedPath,
    pub fingerprint: SourceFingerprint,
    pub messages: Vec<LegacyUnifiedMessageV5>,
    pub fallback_timestamp_indices: Vec<usize>,
    pub codex_incremental: Option<LegacyCodexIncrementalCache>,
}

impl From<LegacyCachedSourceEntryV5> for CachedSourceEntry {
    fn from(entry: LegacyCachedSourceEntryV5) -> Self {
        convert(
            entry.parser_namespace,
            entry.parser_version,
            entry.path,
            entry.fingerprint,
            entry.messages,
            entry.fallback_timestamp_indices,
            entry.codex_incremental,
            entry.prime_accounting,
        )
    }
}

impl From<LegacyCachedSourceEntryV4> for CachedSourceEntry {
    fn from(entry: LegacyCachedSourceEntryV4) -> Self {
        convert(
            entry.parser_namespace,
            entry.parser_version,
            entry.path,
            entry.fingerprint,
            entry.messages,
            entry.fallback_timestamp_indices,
            entry.codex_incremental,
            None,
        )
    }
}

pub(super) fn decode(
    format: u32,
    payload: &[u8],
    limit: u64,
) -> Option<Result<Vec<CachedSourceEntry>, String>> {
    match format {
        FORMAT_V5 => Some(decode_entries::<LegacyCachedSourceEntryV5>(payload, limit)),
        FORMAT_V4 => Some(decode_entries::<LegacyCachedSourceEntryV4>(payload, limit)),
        _ => None,
    }
}

fn decode_entries<T>(payload: &[u8], limit: u64) -> Result<Vec<CachedSourceEntry>, String>
where
    T: serde::de::DeserializeOwned + Into<CachedSourceEntry>,
{
    bincode::options()
        .with_limit(limit)
        .deserialize::<Vec<T>>(payload)
        .map(|entries| entries.into_iter().map(Into::into).collect())
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn convert(
    parser_namespace: String,
    parser_version: u32,
    path: CachedPath,
    fingerprint: SourceFingerprint,
    messages: Vec<LegacyUnifiedMessageV5>,
    fallback_timestamp_indices: Vec<usize>,
    codex_incremental: Option<LegacyCodexIncrementalCache>,
    prime_accounting: Option<crate::sessions::prime_agent::PrimeFileAccounting>,
) -> CachedSourceEntry {
    CachedSourceEntry {
        parser_namespace,
        parser_version,
        path,
        fingerprint,
        messages: messages.into_iter().map(Into::into).collect(),
        fallback_timestamp_indices,
        codex_incremental: codex_incremental.map(Into::into),
        prime_accounting,
    }
}
