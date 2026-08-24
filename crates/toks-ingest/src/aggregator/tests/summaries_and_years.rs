use super::super::*;
use super::mock_unified_message;

#[test]
fn test_calculate_summary_empty() {
    let contributions = Vec::new();
    let summary = calculate_summary(&contributions);

    assert_eq!(summary.total_tokens, 0);
    assert_eq!(summary.total_cost, 0.0);
    assert_eq!(summary.total_days, 0);
    assert_eq!(summary.active_days, 0);
    assert_eq!(summary.average_per_day, 0.0);
    assert_eq!(summary.max_cost_in_single_day, 0.0);
    // `-0.0 == 0.0` under IEEE, so the assertion above cannot catch a
    // negative zero. The CLI formats this straight through, and "$-0.00"
    // is what the user sees when every row was excluded from a submission.
    assert!(
        !summary.total_cost.is_sign_negative(),
        "an empty summary must not carry a negative zero cost"
    );
}

#[test]
fn empty_summary_cost_does_not_format_as_negative_zero() {
    let summary = calculate_summary(&[]);
    assert_eq!(format!("${:.2}", summary.total_cost), "$0.00");
}

/// Pins the boundary the `+ 0.0` comment describes. Only the empty fold
/// (and an all-`-0.0` one) ever reached the `-0.0` identity: a single
/// `+0.0` addend already normalized it, because `-0.0 + 0.0 == +0.0`.
/// Without this, "all-zero" reads as if every zero-cost day was affected.
#[test]
fn a_positive_zero_contribution_was_never_the_negative_zero_case() {
    let messages = vec![mock_unified_message(
        "2024-01-01",
        0,
        0.0,
        "claude-sonnet-4-20250514",
        "claude",
    )];
    let contributions = aggregate_by_date(messages);
    let summary = calculate_summary(&contributions);

    assert!(!summary.total_cost.is_sign_negative());
    assert_eq!(format!("${:.2}", summary.total_cost), "$0.00");
}

#[test]
fn test_calculate_summary_single_day() {
    let messages = vec![mock_unified_message(
        "2024-01-01",
        1000,
        0.05,
        "claude-3-5-sonnet",
        "opencode",
    )];
    let contributions = aggregate_by_date(messages);
    let summary = calculate_summary(&contributions);

    assert_eq!(summary.total_tokens, 1000);
    assert_eq!(summary.total_cost, 0.05);
    assert_eq!(summary.total_days, 1);
    assert_eq!(summary.active_days, 1);
    assert_eq!(summary.average_per_day, 0.05);
    assert_eq!(summary.max_cost_in_single_day, 0.05);
}

#[test]
fn test_calculate_summary_multiple_days() {
    let messages = vec![
        mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-01-02", 2000, 0.10, "gpt-4", "claude"),
        mock_unified_message("2024-01-03", 1500, 0.08, "claude-3-5-sonnet", "opencode"),
    ];
    let contributions = aggregate_by_date(messages);
    let summary = calculate_summary(&contributions);

    assert_eq!(summary.total_tokens, 4500);
    assert!((summary.total_cost - 0.23).abs() < 0.0001);
    assert_eq!(summary.total_days, 3);
    assert_eq!(summary.active_days, 3);
    assert!((summary.average_per_day - 0.23 / 3.0).abs() < 0.0001);
    assert!((summary.max_cost_in_single_day - 0.10).abs() < 0.0001);
}

#[test]
fn test_calculate_summary_with_zero_token_days() {
    let contributions = vec![
        DailyContribution {
            date: "2024-01-01".to_string(),
            totals: DailyTotals {
                tokens: 1000,
                cost: 0.05,
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-02".to_string(),
            totals: DailyTotals {
                tokens: 0,
                cost: 0.0,
                messages: 0,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
    ];

    let summary = calculate_summary(&contributions);
    assert_eq!(summary.total_days, 2);
    assert_eq!(summary.active_days, 1);
    assert!((summary.average_per_day - 0.05).abs() < 0.0001);
}

#[test]
fn test_calculate_summary_counts_cost_only_days_as_active() {
    let contributions = vec![
        DailyContribution {
            date: "2024-01-01".to_string(),
            totals: DailyTotals {
                tokens: 1000,
                cost: 0.05,
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-02".to_string(),
            totals: DailyTotals {
                tokens: 0,
                cost: 1.25,
                messages: 0,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-03".to_string(),
            totals: DailyTotals {
                tokens: 0,
                cost: 0.0,
                messages: 0,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
    ];

    let summary = calculate_summary(&contributions);
    assert_eq!(summary.total_days, 3);
    assert_eq!(summary.active_days, 2);
    assert!((summary.average_per_day - 0.65).abs() < 0.0001);
}

#[test]
fn test_extreme_day_totals_saturate_in_summary_and_years() {
    // Daily totals clamp extreme inputs to i64::MAX; summing several such
    // days must saturate rather than overflow (debug panic / release wrap).
    let saturated_day = |date: &str| DailyContribution {
        date: date.to_string(),
        totals: DailyTotals {
            tokens: i64::MAX,
            cost: 1.0,
            messages: 1,
        },
        intensity: 0,
        token_breakdown: TokenBreakdown::default(),
        clients: Vec::new(),
        active_time_ms: None,
    };
    let contributions = vec![saturated_day("2024-01-01"), saturated_day("2024-01-02")];

    let summary = calculate_summary(&contributions);
    assert_eq!(summary.total_tokens, i64::MAX);

    let years = calculate_years(&contributions);
    assert_eq!(years.len(), 1);
    assert_eq!(years[0].total_tokens, i64::MAX);
}

#[test]
fn test_calculate_years_empty() {
    let contributions = Vec::new();
    let years = calculate_years(&contributions);
    assert_eq!(years.len(), 0);
}

#[test]
fn test_calculate_years_single_year() {
    let messages = vec![
        mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-06-15", 2000, 0.10, "gpt-4", "claude"),
        mock_unified_message("2024-12-31", 1500, 0.08, "claude-3-5-sonnet", "opencode"),
    ];
    let contributions = aggregate_by_date(messages);
    let years = calculate_years(&contributions);

    assert_eq!(years.len(), 1);
    assert_eq!(years[0].year, "2024");
    assert_eq!(years[0].total_tokens, 4500);
    assert!((years[0].total_cost - 0.23).abs() < 0.0001);
    assert_eq!(years[0].range_start, "2024-01-01");
    assert_eq!(years[0].range_end, "2024-12-31");
}

#[test]
fn test_calculate_years_multiple_years() {
    let messages = vec![
        mock_unified_message("2023-12-31", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-01-01", 2000, 0.10, "gpt-4", "claude"),
        mock_unified_message("2024-06-15", 1500, 0.08, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2025-01-01", 3000, 0.15, "gpt-4", "claude"),
    ];
    let contributions = aggregate_by_date(messages);
    let years = calculate_years(&contributions);

    assert_eq!(years.len(), 3);

    // Verify sorted by year
    assert_eq!(years[0].year, "2023");
    assert_eq!(years[1].year, "2024");
    assert_eq!(years[2].year, "2025");

    // Verify 2024 aggregation
    assert_eq!(years[1].total_tokens, 3500);
    assert!((years[1].total_cost - 0.18).abs() < 0.0001);
    assert_eq!(years[1].range_start, "2024-01-01");
    assert_eq!(years[1].range_end, "2024-06-15");
}

#[test]
fn test_calculate_years_year_boundary() {
    let messages = vec![
        mock_unified_message("2024-12-31", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2025-01-01", 2000, 0.10, "gpt-4", "claude"),
    ];
    let contributions = aggregate_by_date(messages);
    let years = calculate_years(&contributions);

    assert_eq!(years.len(), 2);
    assert_eq!(years[0].year, "2024");
    assert_eq!(years[0].total_tokens, 1000);
    assert_eq!(years[1].year, "2025");
    assert_eq!(years[1].total_tokens, 2000);
}

#[test]
fn test_calculate_years_invalid_date() {
    let contributions = vec![DailyContribution {
        date: "abc".to_string(), // Invalid date (less than 4 chars)
        totals: DailyTotals {
            tokens: 1000,
            cost: 0.05,
            messages: 1,
        },
        intensity: 0,
        token_breakdown: TokenBreakdown::default(),
        clients: Vec::new(),
        active_time_ms: None,
    }];

    let years = calculate_years(&contributions);
    assert_eq!(years.len(), 0); // Should skip invalid dates
}
