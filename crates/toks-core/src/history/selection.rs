use std::collections::{BTreeMap, HashMap};

use super::{
    CostCoverage, ModelUsage, SourceHistory, UsageBucket, UsageKey, UsagePeriod, UsageSeries,
};

/// Rebuild aggregate usage from a selected set of provider-client sources.
pub fn merge_source_usage<'a>(sources: impl IntoIterator<Item = &'a SourceHistory>) -> UsageSeries {
    let sources: Vec<_> = sources.into_iter().collect();
    UsageSeries {
        daily: merge_period(
            UsagePeriod::Daily,
            sources.iter().flat_map(|source| &source.usage.daily),
        ),
        hourly: merge_period(
            UsagePeriod::Hourly,
            sources.iter().flat_map(|source| &source.usage.hourly),
        ),
        monthly: merge_period(
            UsagePeriod::Monthly,
            sources.iter().flat_map(|source| &source.usage.monthly),
        ),
    }
}

fn merge_period<'a>(
    period: UsagePeriod,
    buckets: impl IntoIterator<Item = &'a UsageBucket>,
) -> Vec<UsageBucket> {
    let mut totals: BTreeMap<UsageKey, BucketTotals> = BTreeMap::new();
    for bucket in buckets {
        if let Some(key) = UsageKey::parse(period, &bucket.key) {
            totals.entry(key).or_default().add(bucket);
        }
    }
    totals
        .into_iter()
        .map(|(key, total)| total.finish(key.to_string()))
        .collect()
}

#[derive(Default)]
struct BucketTotals {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    messages: i64,
    turns: i64,
    cost: f64,
    cost_coverage: CostCoverage,
    models: HashMap<(String, String), ModelUsage>,
}

impl BucketTotals {
    fn add(&mut self, bucket: &UsageBucket) {
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
            add_model(self.models.entry(model_key(model)).or_default(), model);
        }
    }

    fn finish(self, key: String) -> UsageBucket {
        let mut models: Vec<_> = self.models.into_values().collect();
        models.sort_by(|left, right| {
            right
                .tokens
                .cmp(&left.tokens)
                .then_with(|| left.model.cmp(&right.model))
        });
        let tokens = self
            .input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
            .saturating_add(self.reasoning);
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

fn model_key(model: &ModelUsage) -> (String, String) {
    (model.model.clone(), model.provider.clone())
}

fn add_model(total: &mut ModelUsage, model: &ModelUsage) {
    if total.model.is_empty() {
        total.model.clone_from(&model.model);
        total.provider.clone_from(&model.provider);
    }
    total.input = total.input.saturating_add(model.input);
    total.output = total.output.saturating_add(model.output);
    total.cache_read = total.cache_read.saturating_add(model.cache_read);
    total.cache_write = total.cache_write.saturating_add(model.cache_write);
    total.reasoning = total.reasoning.saturating_add(model.reasoning);
    total.messages = total.messages.saturating_add(model.messages);
    total.turns = total.turns.saturating_add(model.turns);
    total.cost += model.cost;
    total.cost_coverage.add_assign(model.cost_coverage);
    total.tokens = total
        .input
        .saturating_add(total.output)
        .saturating_add(total.cache_read)
        .saturating_add(total.cache_write)
        .saturating_add(total.reasoning);
}
