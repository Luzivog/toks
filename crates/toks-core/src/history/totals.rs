use std::collections::HashMap;

use super::ingress::ValidatedMessage;
use super::{CostCoverage, ModelUsage, UsageBucket};

#[derive(Clone, Default)]
struct ModelTotals {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    messages: i64,
    turns: i64,
    cost: f64,
    cost_coverage: CostCoverage,
}

impl ModelTotals {
    fn add(&mut self, message: &ValidatedMessage<'_>) {
        self.input = self.input.saturating_add(message.input);
        self.output = self.output.saturating_add(message.output);
        self.cache_read = self.cache_read.saturating_add(message.cache_read);
        self.cache_write = self.cache_write.saturating_add(message.cache_write);
        self.reasoning = self.reasoning.saturating_add(message.reasoning);
        self.messages = self.messages.saturating_add(message.messages);
        if message.is_turn_start() {
            self.turns = self.turns.saturating_add(1);
        }
        self.cost += message.cost;
        self.cost_coverage.add_assign(coverage_for_message(message));
    }

    pub(super) fn tokens(&self) -> i64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
            .saturating_add(self.reasoning)
    }

    fn add_usage(&mut self, usage: &ModelUsage) {
        self.input = self.input.saturating_add(usage.input);
        self.output = self.output.saturating_add(usage.output);
        self.cache_read = self.cache_read.saturating_add(usage.cache_read);
        self.cache_write = self.cache_write.saturating_add(usage.cache_write);
        self.reasoning = self.reasoning.saturating_add(usage.reasoning);
        self.messages = self.messages.saturating_add(usage.messages);
        self.turns = self.turns.saturating_add(usage.turns);
        self.cost += usage.cost;
        self.cost_coverage.add_assign(usage.cost_coverage);
    }
}

#[derive(Clone, Default)]
pub(super) struct UsageTotals {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    messages: i64,
    turns: i64,
    cost: f64,
    cost_coverage: CostCoverage,
    models: HashMap<(String, String), ModelTotals>,
}

impl UsageTotals {
    pub(super) fn add(&mut self, message: &ValidatedMessage<'_>) {
        self.input = self.input.saturating_add(message.input);
        self.output = self.output.saturating_add(message.output);
        self.cache_read = self.cache_read.saturating_add(message.cache_read);
        self.cache_write = self.cache_write.saturating_add(message.cache_write);
        self.reasoning = self.reasoning.saturating_add(message.reasoning);
        self.messages = self.messages.saturating_add(message.messages);
        if message.is_turn_start() {
            self.turns = self.turns.saturating_add(1);
        }
        self.cost += message.cost;
        self.cost_coverage.add_assign(coverage_for_message(message));
        let model = self
            .models
            .entry((message.model().to_string(), message.provider().to_string()))
            .or_default();
        model.add(message);
    }

    pub(super) fn tokens(&self) -> i64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
            .saturating_add(self.reasoning)
    }

    pub(super) fn cost(&self) -> f64 {
        self.cost
    }

    pub(super) fn messages(&self) -> i64 {
        self.messages
    }

    pub(super) fn coverage(&self) -> CostCoverage {
        self.cost_coverage
    }

    pub(super) fn model_usage(&self) -> Vec<ModelUsage> {
        let mut models: Vec<_> = self
            .models
            .iter()
            .map(|((model, provider), totals)| ModelUsage {
                model: model.clone(),
                provider: provider.clone(),
                input: totals.input,
                output: totals.output,
                cache_read: totals.cache_read,
                cache_write: totals.cache_write,
                reasoning: totals.reasoning,
                tokens: totals.tokens(),
                messages: totals.messages,
                turns: totals.turns,
                cost: totals.cost,
                cost_coverage: totals.cost_coverage,
            })
            .collect();
        models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
        models
    }

    pub(super) fn add_bucket(&mut self, bucket: &UsageBucket) {
        self.input = self.input.saturating_add(bucket.input);
        self.output = self.output.saturating_add(bucket.output);
        self.cache_read = self.cache_read.saturating_add(bucket.cache_read);
        self.cache_write = self.cache_write.saturating_add(bucket.cache_write);
        self.reasoning = self.reasoning.saturating_add(bucket.reasoning);
        self.messages = self.messages.saturating_add(bucket.messages);
        self.turns = self.turns.saturating_add(bucket.turns);
        self.cost += bucket.cost;
        self.cost_coverage.add_assign(bucket.cost_coverage);
        for model in &bucket.models {
            self.models
                .entry((model.model.clone(), model.provider.clone()))
                .or_default()
                .add_usage(model);
        }
    }

    pub(super) fn into_bucket(self, key: String) -> UsageBucket {
        let tokens = self.tokens();
        let models = self.model_usage();
        UsageBucket {
            key,
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            reasoning: self.reasoning,
            tokens,
            messages: self.messages,
            turns: self.turns,
            cost: self.cost,
            cost_coverage: self.cost_coverage,
            models,
        }
    }
}

fn coverage_for_message(message: &ValidatedMessage<'_>) -> CostCoverage {
    let mut coverage = CostCoverage {
        invalid_records: i64::from(message.invalid_metrics),
        ..Default::default()
    };
    if message.cost_is_covered() {
        coverage.covered_tokens = message.tokens();
        coverage.covered_messages = message.messages;
    } else {
        coverage.uncovered_tokens = message.tokens();
        coverage.uncovered_messages = message.messages;
    }
    coverage
}
