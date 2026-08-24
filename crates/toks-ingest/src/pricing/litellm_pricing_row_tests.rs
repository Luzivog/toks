use super::ModelPricing;
use crate::TokenBreakdown;

fn cache_read_usage() -> TokenBreakdown {
    TokenBreakdown {
        input: 10,
        output: 0,
        cache_read: 10,
        cache_write: 0,
        reasoning: 0,
    }
}

// A hinted row can publish a long-context tier for a bucket whose base
// rate it omits. Filling the base must not drag the fallback's tier in
// with it, or long-context usage silently reprices onto another row.
#[test]
fn existing_long_context_tiers_survive_a_filled_base_rate() {
    let hinted = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        output_cost_per_token: Some(1.4e-5),
        cache_read_input_token_cost_above_200k_tokens: Some(5e-7),
        ..Default::default()
    };
    let fallback = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        output_cost_per_token: Some(1.4e-5),
        cache_read_input_token_cost: Some(1.75e-7),
        cache_read_input_token_cost_above_200k_tokens: Some(9.9e-7),
        ..Default::default()
    };

    let filled = hinted.with_missing_rates_from(&fallback, &cache_read_usage());

    assert_eq!(filled.cache_read_input_token_cost, Some(1.75e-7));
    assert_eq!(
        filled.cache_read_input_token_cost_above_200k_tokens,
        Some(5e-7),
        "the hinted row's own long-context tier must be preserved"
    );
}

// Absent tiers are still worth filling, otherwise a borrowed base rate
// walks off a cliff once usage crosses the threshold.
#[test]
fn absent_long_context_tiers_are_filled_alongside_the_base_rate() {
    let hinted = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        output_cost_per_token: Some(1.4e-5),
        ..Default::default()
    };
    let fallback = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        output_cost_per_token: Some(1.4e-5),
        cache_read_input_token_cost: Some(1.75e-7),
        cache_read_input_token_cost_above_200k_tokens: Some(9.9e-7),
        ..Default::default()
    };

    let filled = hinted.with_missing_rates_from(&fallback, &cache_read_usage());

    assert_eq!(filled.cache_read_input_token_cost, Some(1.75e-7));
    assert_eq!(
        filled.cache_read_input_token_cost_above_200k_tokens,
        Some(9.9e-7)
    );
}

#[test]
fn invalid_long_context_tiers_fall_back_to_valid_tiers() {
    let hinted = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        output_cost_per_token: Some(1.4e-5),
        cache_read_input_token_cost_above_200k_tokens: Some(f64::NAN),
        ..Default::default()
    };
    let fallback = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        output_cost_per_token: Some(1.4e-5),
        cache_read_input_token_cost: Some(1.75e-7),
        cache_read_input_token_cost_above_200k_tokens: Some(9.9e-7),
        ..Default::default()
    };

    let filled = hinted.with_missing_rates_from(&fallback, &cache_read_usage());

    assert_eq!(
        filled.cache_read_input_token_cost_above_200k_tokens,
        Some(9.9e-7)
    );
}

fn every_bucket_usage() -> TokenBreakdown {
    TokenBreakdown {
        input: 1_000,
        output: 500,
        cache_read: 2_000,
        cache_write: 300,
        reasoning: 200,
    }
}

// #1021, #1035: a free model whose row omits the redundant cache zeros was
// judged unpriced the moment a message carried one cached token, and the
// whole submission aborted.
#[test]
fn a_row_priced_entirely_at_zero_covers_cache_usage_it_never_quotes() {
    let free = ModelPricing {
        input_cost_per_token: Some(0.0),
        output_cost_per_token: Some(0.0),
        ..Default::default()
    };

    assert!(free.covers_usage(&every_bucket_usage()));
}

// Absence of data is not a price of zero.
#[test]
fn a_row_with_no_rates_at_all_covers_nothing() {
    let empty = ModelPricing::default();

    assert!(!empty.covers_usage(&every_bucket_usage()));
    assert!(!empty.covers_usage(&cache_read_usage()));
}

// The zero shortcut must never borrow a real rate for a bucket the row
// leaves unquoted: that would bill cached tokens at the input price.
#[test]
fn a_row_charging_for_input_still_does_not_cover_unquoted_cache_reads() {
    let paid = ModelPricing {
        input_cost_per_token: Some(1e-6),
        output_cost_per_token: Some(1e-5),
        ..Default::default()
    };

    assert!(!paid.covers_usage(&cache_read_usage()));
}

// A zero base rate beside a paid long-context tier is not an all-zero row,
// so the strict rule still applies to the buckets it never quotes.
#[test]
fn a_zero_base_rate_with_a_paid_tier_does_not_cover_unquoted_cache_reads() {
    let promotional = ModelPricing {
        input_cost_per_token: Some(0.0),
        input_cost_per_token_above_128k_tokens: Some(1e-6),
        output_cost_per_token: Some(0.0),
        ..Default::default()
    };

    assert!(!promotional.covers_usage(&cache_read_usage()));
}

// One zero rate is not enough. A row quoting zero input while saying
// nothing about output has said nothing about generation, so the buckets
// it omits stay unpriced.
#[test]
fn a_row_quoting_only_a_zero_input_rate_does_not_cover_output() {
    let input_only = ModelPricing {
        input_cost_per_token: Some(0.0),
        ..Default::default()
    };

    assert!(!input_only.covers_usage(&every_bucket_usage()));
    assert!(input_only.covers_usage(&TokenBreakdown {
        input: 1_000,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    }));
}

// A tier-only row has no base rate to anchor its zeros, so they
// must not make it cover usage the pricing path would bill at zero anyway.
#[test]
fn a_tier_only_zero_row_covers_nothing() {
    let tier_only = ModelPricing {
        input_cost_per_token_above_128k_tokens: Some(0.0),
        ..Default::default()
    };

    assert!(!tier_only.covers_usage(&every_bucket_usage()));
}

// Covering the usage is only useful if the price that follows is a real
// 0.0: an unquoted bucket must not leak a NaN into the leaderboard totals.
#[test]
fn an_all_zero_row_prices_cache_usage_at_exactly_zero() {
    let free = ModelPricing {
        input_cost_per_token: Some(0.0),
        output_cost_per_token: Some(0.0),
        ..Default::default()
    };
    let usage = every_bucket_usage();

    let cost = crate::pricing::lookup::compute_cost(
        &free,
        usage.input,
        usage.output,
        usage.cache_read,
        usage.cache_write,
        usage.reasoning,
    );

    assert_eq!(cost, 0.0);
    assert!(cost.is_finite());
}

// A bucket the usage does not touch is never filled.
#[test]
fn untouched_buckets_are_left_alone() {
    let hinted = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        ..Default::default()
    };
    let fallback = ModelPricing {
        input_cost_per_token: Some(1.75e-6),
        cache_creation_input_token_cost: Some(2e-6),
        ..Default::default()
    };

    let filled = hinted.with_missing_rates_from(&fallback, &cache_read_usage());

    assert_eq!(filled.cache_creation_input_token_cost, None);
}
