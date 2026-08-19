use std::collections::HashMap;

use chrono::{Local, NaiveDate, TimeZone};
use tokscope_core::history::{HistorySnapshot, ModelUsage, UsageKey, UsagePeriod};

use crate::{ModelSortColumn, SortDirection, SortState};

pub(super) fn aggregate_model_usage<'a>(
    models: impl IntoIterator<Item = &'a ModelUsage>,
) -> Vec<ModelUsage> {
    let mut totals: HashMap<(String, String), ModelUsage> = HashMap::new();
    for model in models {
        let total = totals
            .entry((model.model.clone(), model.provider.clone()))
            .or_insert_with(|| ModelUsage {
                model: model.model.clone(),
                provider: model.provider.clone(),
                ..Default::default()
            });
        total.input = total.input.saturating_add(model.input);
        total.output = total.output.saturating_add(model.output);
        total.cache_read = total.cache_read.saturating_add(model.cache_read);
        total.cache_write = total.cache_write.saturating_add(model.cache_write);
        total.reasoning = total.reasoning.saturating_add(model.reasoning);
        total.tokens = total.tokens.saturating_add(model.tokens);
        total.messages = total.messages.saturating_add(model.messages);
        total.turns = total.turns.saturating_add(model.turns);
        total.cost += model.cost;
        total.cost_coverage.covered_tokens = total
            .cost_coverage
            .covered_tokens
            .saturating_add(model.cost_coverage.covered_tokens);
        total.cost_coverage.uncovered_tokens = total
            .cost_coverage
            .uncovered_tokens
            .saturating_add(model.cost_coverage.uncovered_tokens);
        total.cost_coverage.covered_messages = total
            .cost_coverage
            .covered_messages
            .saturating_add(model.cost_coverage.covered_messages);
        total.cost_coverage.uncovered_messages = total
            .cost_coverage
            .uncovered_messages
            .saturating_add(model.cost_coverage.uncovered_messages);
        total.cost_coverage.invalid_records = total
            .cost_coverage
            .invalid_records
            .saturating_add(model.cost_coverage.invalid_records);
    }
    let mut models: Vec<_> = totals
        .into_values()
        .filter(|model| model.tokens > 0)
        .collect();
    models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
    models
}

pub(super) fn current_usage_date(history: &HistorySnapshot) -> NaiveDate {
    let generated = if history.generated_at_ms > 0 {
        Local
            .timestamp_millis_opt(history.generated_at_ms)
            .single()
            .map(|time| time.date_naive())
    } else {
        None
    };
    generated
        .or_else(|| {
            history
                .sources
                .iter()
                .filter_map(|source| {
                    source
                        .days
                        .last()
                        .and_then(|day| NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").ok())
                })
                .max()
        })
        .or_else(|| {
            history
                .usage
                .daily
                .iter()
                .filter_map(|bucket| match bucket.typed_key(UsagePeriod::Daily) {
                    Some(UsageKey::Daily(date)) => Some(date),
                    _ => None,
                })
                .max()
        })
        .unwrap_or_else(|| Local::now().date_naive())
}

pub(super) fn period_model_usage(
    history: &HistorySnapshot,
    period: UsagePeriod,
) -> Vec<ModelUsage> {
    match period {
        UsagePeriod::Hourly => aggregate_model_usage(
            history
                .sources
                .iter()
                .flat_map(|source| &source.minutes)
                .flat_map(|minute| &minute.models),
        ),
        UsagePeriod::Daily => {
            let key = current_usage_date(history).format("%Y-%m-%d").to_string();
            aggregate_model_usage(
                history
                    .usage
                    .daily
                    .iter()
                    .filter(|bucket| bucket.key == key)
                    .flat_map(|bucket| &bucket.models),
            )
        }
        UsagePeriod::Monthly => {
            let key = current_usage_date(history).format("%Y-%m").to_string();
            aggregate_model_usage(
                history
                    .usage
                    .monthly
                    .iter()
                    .filter(|bucket| bucket.key == key)
                    .flat_map(|bucket| &bucket.models),
            )
        }
    }
}

pub(super) fn sort_model_usage(models: &mut [ModelUsage], sort: SortState<ModelSortColumn>) {
    let Some(column) = sort.column else {
        return;
    };
    models.sort_by(|a, b| {
        let order = match column {
            ModelSortColumn::Input => a.input.cmp(&b.input),
            ModelSortColumn::CacheRead => a.cache_read.cmp(&b.cache_read),
            ModelSortColumn::CacheWrite => a.cache_write.cmp(&b.cache_write),
            ModelSortColumn::Output => a.output.cmp(&b.output),
            ModelSortColumn::Reasoning => a.reasoning.cmp(&b.reasoning),
            ModelSortColumn::Messages => a.messages.cmp(&b.messages),
            ModelSortColumn::Turns => a.turns.cmp(&b.turns),
            ModelSortColumn::Total => a.tokens.cmp(&b.tokens),
            ModelSortColumn::Cost => a.cost.total_cmp(&b.cost),
        };
        match sort.direction {
            SortDirection::Ascending => order,
            SortDirection::Descending => order.reverse(),
        }
        .then_with(|| a.model.cmp(&b.model))
    });
}
