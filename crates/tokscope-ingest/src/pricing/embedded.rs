//! Deterministic pricing used before optional remote catalogs are available.
//!
//! These provider-direct rates are intentionally small in scope: they cover
//! the OpenAI and Anthropic models Tokscope currently ingests locally. Remote
//! catalogs and the user's custom pricing file may replace these rows, but a
//! first local snapshot never depends on network access.

use super::ModelPricing;
use std::collections::HashMap;

type RateRow = (&'static str, f64, f64, Option<f64>, Option<f64>);

// Provider-published standard prices, expressed per token. Last audited
// 2026-08-18. Sonnet 5's row uses its introductory price through 2026-08-31;
// background catalog refreshes supersede this conservative bootstrap table.
const RATES: &[RateRow] = &[
    (
        "openai/gpt-5.6-sol",
        5e-6,
        30e-6,
        Some(0.5e-6),
        Some(6.25e-6),
    ),
    (
        "openai/gpt-5.6-terra",
        2e-6,
        12e-6,
        Some(0.2e-6),
        Some(2.5e-6),
    ),
    (
        "openai/gpt-5.6-luna",
        0.2e-6,
        1.2e-6,
        Some(0.02e-6),
        Some(0.25e-6),
    ),
    ("openai/gpt-5.5", 5e-6, 30e-6, Some(0.5e-6), None),
    ("openai/gpt-5.4", 2.5e-6, 15e-6, Some(0.25e-6), None),
    ("openai/gpt-5.3-codex", 1.75e-6, 14e-6, Some(0.175e-6), None),
    (
        "openai/gpt-5.3-codex-spark",
        1.75e-6,
        14e-6,
        Some(0.175e-6),
        None,
    ),
    (
        "anthropic/claude-sonnet-5",
        2e-6,
        10e-6,
        Some(0.2e-6),
        Some(2.5e-6),
    ),
    (
        "anthropic/claude-opus-5",
        5e-6,
        25e-6,
        Some(0.5e-6),
        Some(6.25e-6),
    ),
    (
        "anthropic/claude-fable-5",
        10e-6,
        50e-6,
        Some(1e-6),
        Some(12.5e-6),
    ),
];

pub(super) fn dataset() -> HashMap<String, ModelPricing> {
    RATES
        .iter()
        .map(|(model, input, output, cache_read, cache_write)| {
            (
                (*model).to_string(),
                ModelPricing {
                    input_cost_per_token: Some(*input),
                    output_cost_per_token: Some(*output),
                    cache_read_input_token_cost: *cache_read,
                    cache_creation_input_token_cost: *cache_write,
                    ..Default::default()
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::dataset;

    #[test]
    fn baseline_has_usable_openai_and_anthropic_rows() {
        let prices = dataset();

        for model in ["openai/gpt-5.6-sol", "anthropic/claude-fable-5"] {
            let row = prices.get(model).expect("baseline model must be present");
            assert!(row.has_any_usable_base_rate());
            assert!(row.cache_read_input_token_cost.is_some());
        }
    }
}
