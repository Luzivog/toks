use toks_ingest::sessions::{CostSource, UnifiedMessage};

/// Non-negative, finite metrics admitted into history aggregation.
pub(super) struct ValidatedMessage<'a> {
    source: &'a UnifiedMessage,
    pub(super) input: i64,
    pub(super) output: i64,
    pub(super) cache_read: i64,
    pub(super) cache_write: i64,
    pub(super) reasoning: i64,
    pub(super) messages: i64,
    pub(super) cost: f64,
    pub(super) invalid_metrics: bool,
}

impl<'a> ValidatedMessage<'a> {
    pub(super) fn new(source: &'a UnifiedMessage) -> Self {
        let cost_is_valid = source.cost.is_finite() && source.cost >= 0.0;
        let invalid_metrics = source.tokens.input < 0
            || source.tokens.output < 0
            || source.tokens.cache_read < 0
            || source.tokens.cache_write < 0
            || source.tokens.reasoning < 0
            || source.message_count < 0
            || !cost_is_valid;
        Self {
            source,
            input: source.tokens.input.max(0),
            output: source.tokens.output.max(0),
            cache_read: source.tokens.cache_read.max(0),
            cache_write: source.tokens.cache_write.max(0),
            reasoning: source.tokens.reasoning.max(0),
            messages: i64::from(source.message_count.max(0)),
            cost: if cost_is_valid { source.cost } else { 0.0 },
            invalid_metrics,
        }
    }

    pub(super) fn tokens(&self) -> i64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
            .saturating_add(self.reasoning)
    }

    pub(super) fn cost_is_covered(&self) -> bool {
        !self.invalid_metrics
            && (self.tokens() == 0 || self.source.cost_source != CostSource::Unknown)
    }

    pub(super) fn model(&self) -> &str {
        &self.source.model_id
    }

    pub(super) fn provider(&self) -> &str {
        &self.source.provider_id
    }

    pub(super) fn date(&self) -> &str {
        &self.source.date
    }

    pub(super) fn timestamp(&self) -> i64 {
        self.source.timestamp
    }

    pub(super) fn is_turn_start(&self) -> bool {
        self.source.is_turn_start
    }
}
