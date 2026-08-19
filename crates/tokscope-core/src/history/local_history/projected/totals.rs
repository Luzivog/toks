use std::collections::BTreeMap;

use tokscope_ingest::pricing::PricingService;
use tokscope_ingest::TokenBreakdown;

use crate::history::archive::ArchiveRollup;
use crate::history::{CostCoverage, ModelUsage, UsageBucket};

#[derive(Clone, Default)]
struct Values {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    messages: i64,
    turns: i64,
    cost: f64,
    coverage: CostCoverage,
}

impl Values {
    fn add(&mut self, row: &ArchiveRollup, cost: f64, covered: bool) {
        self.input = self.input.saturating_add(row.input);
        self.output = self.output.saturating_add(row.output);
        self.cache_read = self.cache_read.saturating_add(row.cache_read);
        self.cache_write = self.cache_write.saturating_add(row.cache_write);
        self.reasoning = self.reasoning.saturating_add(row.reasoning);
        self.messages = self.messages.saturating_add(row.messages);
        self.turns = self.turns.saturating_add(row.turns);
        self.cost += cost;
        let tokens = row_tokens(row);
        if covered {
            self.coverage.covered_tokens = self.coverage.covered_tokens.saturating_add(tokens);
            self.coverage.covered_messages =
                self.coverage.covered_messages.saturating_add(row.messages);
        } else {
            self.coverage.uncovered_tokens = self.coverage.uncovered_tokens.saturating_add(tokens);
            self.coverage.uncovered_messages = self
                .coverage
                .uncovered_messages
                .saturating_add(row.messages);
        }
    }

    fn tokens(&self) -> i64 {
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
        self.coverage.add_assign(usage.cost_coverage);
    }
}

#[derive(Clone, Default)]
pub(super) struct Totals {
    values: Values,
    models: BTreeMap<(String, String), Values>,
}

impl Totals {
    pub(super) fn add(&mut self, row: &ArchiveRollup, pricing: Option<&PricingService>) {
        if row.event_count <= 0 {
            return;
        }
        let (cost, covered) = cost_projection(row, pricing);
        self.values.add(row, cost, covered);
        self.models
            .entry((row.model.clone(), row.provider.clone()))
            .or_default()
            .add(row, cost, covered);
    }

    pub(super) fn add_bucket(&mut self, bucket: &UsageBucket) {
        self.values.input = self.values.input.saturating_add(bucket.input);
        self.values.output = self.values.output.saturating_add(bucket.output);
        self.values.cache_read = self.values.cache_read.saturating_add(bucket.cache_read);
        self.values.cache_write = self.values.cache_write.saturating_add(bucket.cache_write);
        self.values.reasoning = self.values.reasoning.saturating_add(bucket.reasoning);
        self.values.messages = self.values.messages.saturating_add(bucket.messages);
        self.values.turns = self.values.turns.saturating_add(bucket.turns);
        self.values.cost += bucket.cost;
        self.values.coverage.add_assign(bucket.cost_coverage);
        for model in &bucket.models {
            self.models
                .entry((model.model.clone(), model.provider.clone()))
                .or_default()
                .add_usage(model);
        }
    }

    pub(super) fn tokens(&self) -> i64 {
        self.values.tokens()
    }

    pub(super) fn cost(&self) -> f64 {
        self.values.cost
    }

    pub(super) fn messages(&self) -> i64 {
        self.values.messages
    }

    pub(super) fn coverage(&self) -> CostCoverage {
        self.values.coverage
    }

    pub(super) fn model_usage(&self) -> Vec<ModelUsage> {
        let mut models: Vec<_> = self
            .models
            .iter()
            .map(|((model, provider), values)| ModelUsage {
                model: model.clone(),
                provider: provider.clone(),
                input: values.input,
                output: values.output,
                cache_read: values.cache_read,
                cache_write: values.cache_write,
                reasoning: values.reasoning,
                tokens: values.tokens(),
                messages: values.messages,
                turns: values.turns,
                cost: values.cost,
                cost_coverage: values.coverage,
            })
            .collect();
        models.sort_by(|left, right| {
            right
                .tokens
                .cmp(&left.tokens)
                .then_with(|| left.model.cmp(&right.model))
        });
        models
    }

    pub(super) fn into_bucket(self, key: String) -> UsageBucket {
        UsageBucket {
            key,
            input: self.values.input,
            output: self.values.output,
            cache_read: self.values.cache_read,
            cache_write: self.values.cache_write,
            reasoning: self.values.reasoning,
            tokens: self.values.tokens(),
            messages: self.values.messages,
            turns: self.values.turns,
            cost: self.values.cost,
            cost_coverage: self.values.coverage,
            models: self.model_usage(),
        }
    }
}

fn cost_projection(row: &ArchiveRollup, pricing: Option<&PricingService>) -> (f64, bool) {
    if row.cost_source == 2 {
        return (row.cost_nanos as f64 / 1_000_000_000.0, true);
    }
    let tokens = TokenBreakdown {
        input: row.input,
        output: row.output,
        cache_read: row.cache_read,
        cache_write: row.cache_write,
        reasoning: row.reasoning,
    };
    let Some(pricing) = pricing else {
        return (0.0, row_tokens(row) == 0);
    };
    let provider = (!row.provider.is_empty()).then_some(row.provider.as_str());
    let cost = pricing.calculate_basis_cost_with_provider(
        &row.model,
        provider,
        &tokens,
        &row.pricing_basis,
        row.long_context,
    );
    let covered = pricing.covers_usage_with_provider(&row.model, provider, &tokens);
    (if cost.is_finite() { cost.max(0.0) } else { 0.0 }, covered)
}

fn row_tokens(row: &ArchiveRollup) -> i64 {
    row.input
        .saturating_add(row.output)
        .saturating_add(row.cache_read)
        .saturating_add(row.cache_write)
        .saturating_add(row.reasoning)
}
