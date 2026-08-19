use anyhow::{bail, Result};
use rusqlite::Row;
use toks_ingest::sessions::{CostSource, UnifiedMessage};

use super::identity;

/// Version of the accounting projection persisted in each revision.
///
/// Changing this requires an explicit, tested conflict-resolution migration;
/// a newer projection must never silently replace accepted historical facts.
pub(super) const ACCOUNTING_PROJECTION_VERSION: i64 = 2;

#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub accounting_hash: String,
    pub accounting_projection_version: i64,
    pub fact_hash: String,
    pub client: String,
    pub provider: String,
    pub model: String,
    pub timestamp_ms: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub duration_ms: Option<i64>,
    pub message_count: i64,
    pub is_turn_start: bool,
    pub model_conflicted: bool,
    pub cost_nanos: i64,
    pub cost_source: i64,
    pub first_observed_generation: i64,
}

impl Candidate {
    pub fn from_message(message: &UnifiedMessage, scan_generation: i64) -> Result<Self> {
        if scan_generation <= 0 || message.timestamp < 0 {
            bail!("usage observation has an invalid timestamp");
        }
        let values = [
            message.tokens.input,
            message.tokens.output,
            message.tokens.cache_read,
            message.tokens.cache_write,
            message.tokens.reasoning,
            i64::from(message.message_count),
        ];
        if values.iter().any(|value| *value < 0) {
            bail!("usage observation has negative accounting facts");
        }
        if message.client.is_empty() || message.model_id.is_empty() {
            bail!("usage observation is missing its client or model");
        }

        let (cost_nanos, cost_source) = normalized_cost(message.cost, message.cost_source);
        let mut candidate = Self {
            accounting_hash: String::new(),
            accounting_projection_version: ACCOUNTING_PROJECTION_VERSION,
            fact_hash: String::new(),
            client: message.client.clone(),
            provider: message.provider_id.clone(),
            model: message.model_id.clone(),
            timestamp_ms: message.timestamp,
            input: message.tokens.input,
            output: message.tokens.output,
            cache_read: message.tokens.cache_read,
            cache_write: message.tokens.cache_write,
            reasoning: message.tokens.reasoning,
            duration_ms: message.duration_ms.filter(|duration| *duration >= 0),
            message_count: i64::from(message.message_count),
            is_turn_start: message.is_turn_start,
            model_conflicted: message.model_attribution_conflicted,
            cost_nanos,
            cost_source,
            first_observed_generation: scan_generation,
        };
        candidate.accounting_hash = candidate.calculate_accounting_hash();
        candidate.fact_hash = candidate.calculate_revision_hash();
        Ok(candidate)
    }

    pub fn from_revision(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            fact_hash: row.get(0)?,
            accounting_hash: row.get(1)?,
            accounting_projection_version: row.get(2)?,
            client: row.get(3)?,
            provider: row.get(4)?,
            model: row.get(5)?,
            timestamp_ms: row.get(6)?,
            input: row.get(7)?,
            output: row.get(8)?,
            cache_read: row.get(9)?,
            cache_write: row.get(10)?,
            reasoning: row.get(11)?,
            duration_ms: row.get(12)?,
            message_count: row.get(13)?,
            is_turn_start: row.get::<_, i64>(14)? != 0,
            model_conflicted: row.get::<_, i64>(15)? != 0,
            cost_nanos: row.get(16)?,
            cost_source: row.get(17)?,
            first_observed_generation: row.get(18)?,
        })
    }

    pub fn is_monotonic_extension_of(&self, earlier: &Self) -> bool {
        self.accounting_projection_version == earlier.accounting_projection_version
            && compatible_client(&self.client, &earlier.client)
            && self.provider == earlier.provider
            && self.model == earlier.model
            && self.timestamp_ms == earlier.timestamp_ms
            && self.model_conflicted == earlier.model_conflicted
            && self.input >= earlier.input
            && self.output >= earlier.output
            && self.cache_read >= earlier.cache_read
            && self.cache_write >= earlier.cache_write
            && self.reasoning >= earlier.reasoning
            && self.message_count >= earlier.message_count
            && (!earlier.is_turn_start || self.is_turn_start)
            && duration_rank(self.duration_ms) >= duration_rank(earlier.duration_ms)
            && cost_provenance_extends(self, earlier)
    }

    fn calculate_accounting_hash(&self) -> String {
        identity::fact_hash([
            "accounting-fact".into(),
            self.accounting_projection_version.to_string(),
            self.client.clone(),
            self.provider.clone(),
            self.model.clone(),
            self.timestamp_ms.to_string(),
            self.input.to_string(),
            self.output.to_string(),
            self.cache_read.to_string(),
            self.cache_write.to_string(),
            self.reasoning.to_string(),
            self.duration_ms.unwrap_or(-1).to_string(),
            self.message_count.to_string(),
            i64::from(self.is_turn_start).to_string(),
            i64::from(self.model_conflicted).to_string(),
        ])
    }

    fn calculate_revision_hash(&self) -> String {
        let reported_cost = if self.cost_source == 2 {
            self.cost_nanos.to_string()
        } else {
            "no-reported-cost".into()
        };
        identity::fact_hash([
            "revision-v1".into(),
            self.accounting_projection_version.to_string(),
            self.accounting_hash.clone(),
            reported_cost,
        ])
    }
}

fn compatible_client(left: &str, right: &str) -> bool {
    left == right || claude_domain(left) && claude_domain(right)
}

fn claude_domain(client: &str) -> bool {
    client == "claude" || client.starts_with("cc-mirror/")
}

fn duration_rank(duration: Option<i64>) -> (bool, i64) {
    (duration.is_some(), duration.unwrap_or_default())
}

fn cost_provenance_extends(later: &Candidate, earlier: &Candidate) -> bool {
    earlier.cost_source == 0 || later.cost_source == 2 && later.cost_nanos == earlier.cost_nanos
}

fn normalized_cost(cost: f64, source: CostSource) -> (i64, i64) {
    if source != CostSource::ProviderReported || !cost.is_finite() || cost < 0.0 {
        return (0, 0);
    }
    let nanos = cost * 1_000_000_000.0;
    if nanos > i64::MAX as f64 {
        return (0, 0);
    }
    (nanos.round() as i64, 2)
}
