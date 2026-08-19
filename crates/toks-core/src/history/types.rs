use serde::{Deserialize, Serialize};

use super::{UsageKey, UsageRange};

/// How much token-bearing history has a trustworthy cost value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CostCoverage {
    pub covered_tokens: i64,
    pub uncovered_tokens: i64,
    pub covered_messages: i64,
    pub uncovered_messages: i64,
    /// Source records whose negative or non-finite metrics were clamped.
    pub invalid_records: i64,
}

impl CostCoverage {
    pub fn is_complete(self) -> bool {
        self.uncovered_tokens == 0
    }

    pub fn covered_ratio(self) -> f64 {
        let total = self.covered_tokens.saturating_add(self.uncovered_tokens);
        if total == 0 {
            1.0
        } else {
            self.covered_tokens as f64 / total as f64
        }
    }

    pub(crate) fn add_assign(&mut self, other: Self) {
        self.covered_tokens = self.covered_tokens.saturating_add(other.covered_tokens);
        self.uncovered_tokens = self.uncovered_tokens.saturating_add(other.uncovered_tokens);
        self.covered_messages = self.covered_messages.saturating_add(other.covered_messages);
        self.uncovered_messages = self
            .uncovered_messages
            .saturating_add(other.uncovered_messages);
        self.invalid_records = self.invalid_records.saturating_add(other.invalid_records);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MinuteSlice {
    /// Unix minute (epoch seconds / 60).
    pub minute: i64,
    pub tokens: i64,
    pub cost: f64,
    pub models: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DaySlice {
    pub date: String, // YYYY-MM-DD
    pub tokens: i64,
    pub cost: f64,
    pub messages: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UsagePeriod {
    #[default]
    Daily,
    Hourly,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUsage {
    pub model: String,
    pub provider: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub tokens: i64,
    pub messages: i64,
    pub turns: i64,
    pub cost: f64,
    pub cost_coverage: CostCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageBucket {
    /// `YYYY-MM-DD`, `YYYY-MM-DD HH:00`, or `YYYY-MM` for the selected period.
    pub key: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub tokens: i64,
    pub messages: i64,
    pub turns: i64,
    pub cost: f64,
    pub cost_coverage: CostCoverage,
    /// Models active in this period, sorted by tokens descending.
    pub models: Vec<ModelUsage>,
}

impl UsageBucket {
    pub fn typed_key(&self, period: UsagePeriod) -> Option<UsageKey> {
        UsageKey::parse(period, &self.key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageSeries {
    pub daily: Vec<UsageBucket>,
    pub hourly: Vec<UsageBucket>,
    pub monthly: Vec<UsageBucket>,
}

impl UsageSeries {
    pub fn buckets(&self, period: UsagePeriod) -> &[UsageBucket] {
        match period {
            UsagePeriod::Daily => &self.daily,
            UsagePeriod::Hourly => &self.hourly,
            UsagePeriod::Monthly => &self.monthly,
        }
    }

    /// Buckets in an inclusive, typed range. Malformed legacy keys are skipped.
    pub fn query(&self, range: UsageRange) -> Vec<&UsageBucket> {
        self.buckets(range.period())
            .iter()
            .filter(|bucket| {
                bucket
                    .typed_key(range.period())
                    .is_some_and(|key| range.contains(key))
            })
            .collect()
    }
}

/// Compatibility name for the all-time model rows used by older callers.
pub type ModelRow = ModelUsage;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceHistory {
    pub client: String,
    /// Last 60 minutes, oldest → newest, gaps zero-filled.
    pub minutes: Vec<MinuteSlice>,
    /// Last 30 days, oldest → newest, gaps zero-filled.
    pub days: Vec<DaySlice>,
    /// Active usage buckets, oldest → newest, for the in-app period switcher.
    pub usage: UsageSeries,
    /// Sorted by cost, descending.
    /// Compatibility projection of the all-time usage models.
    pub models: Vec<ModelRow>,
    // Compatibility totals derived from the same all-time usage accumulator.
    pub today_tokens: i64,
    pub today_cost: f64,
    pub week_cost: f64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub total_messages: i64,
    pub cost_coverage: CostCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistorySnapshot {
    pub sources: Vec<SourceHistory>,
    /// Usage across every tracked provider and account.
    pub usage: UsageSeries,
    pub generated_at_ms: i64,
    /// First time an accounting event was durably accepted by Toks.
    #[serde(default)]
    pub captured_since_ms: Option<i64>,
    /// Most recent successful archive observation. While history is catching
    /// up, older provider sources can still remain unindexed.
    #[serde(default)]
    pub captured_through_ms: Option<i64>,
    /// Durable events backed by a provider/parser identity.
    #[serde(default)]
    pub strong_events: i64,
    /// Durable events accepted through a conservative fallback identity.
    #[serde(default)]
    pub weak_events: i64,
    /// Contradictory observations retained for diagnosis.
    #[serde(default)]
    pub history_conflicts: i64,
    pub cost_coverage: CostCoverage,
    /// Compatibility flag: token-bearing usage without verified cost coverage.
    pub unpriced: bool,
}

impl HistorySnapshot {
    pub fn source(&self, client: &str) -> Option<&SourceHistory> {
        self.sources.iter().find(|s| s.client == client)
    }
}
