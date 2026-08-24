use super::super::*;
use super::mock_unified_message;

#[test]
fn test_generate_graph_result_empty() {
    let contributions = Vec::new();
    let result = generate_graph_result(contributions, 100);

    assert_eq!(result.contributions.len(), 0);
    assert_eq!(result.summary.total_tokens, 0);
    assert_eq!(result.years.len(), 0);
    assert_eq!(result.meta.processing_time_ms, 100);
    assert_eq!(result.meta.date_range_start, "");
    assert_eq!(result.meta.date_range_end, "");
}

#[test]
fn test_generate_graph_result_with_data() {
    let messages = vec![
        mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-01-02", 2000, 0.10, "gpt-4", "claude"),
    ];
    let contributions = aggregate_by_date(messages);
    let result = generate_graph_result(contributions, 150);

    assert_eq!(result.contributions.len(), 2);
    assert_eq!(result.summary.total_tokens, 3000);
    assert_eq!(result.years.len(), 1);
    assert_eq!(result.meta.processing_time_ms, 150);
    assert_eq!(result.meta.date_range_start, "2024-01-01");
    assert_eq!(result.meta.date_range_end, "2024-01-02");
    assert_eq!(result.meta.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn test_calculate_intensities_empty() {
    let mut contributions = Vec::new();
    calculate_intensities(&mut contributions);
    assert_eq!(contributions.len(), 0);
}

#[test]
fn test_calculate_intensities_zero_cost() {
    let mut contributions = vec![
        DailyContribution {
            date: "2024-01-01".to_string(),
            totals: DailyTotals {
                tokens: 1000,
                cost: 0.0,
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
                tokens: 2000,
                cost: 0.0,
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
    ];

    calculate_intensities(&mut contributions);
    assert_eq!(contributions[0].intensity, 0);
    assert_eq!(contributions[1].intensity, 0);
}

#[test]
fn test_calculate_intensities_levels() {
    let mut contributions = vec![
        DailyContribution {
            date: "2024-01-01".to_string(),
            totals: DailyTotals {
                tokens: 1000,
                cost: 1.0, // 100% of max
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
                tokens: 800,
                cost: 0.8, // 80% of max (>= 0.75)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-03".to_string(),
            totals: DailyTotals {
                tokens: 600,
                cost: 0.6, // 60% of max (>= 0.5)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-04".to_string(),
            totals: DailyTotals {
                tokens: 300,
                cost: 0.3, // 30% of max (>= 0.25)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-05".to_string(),
            totals: DailyTotals {
                tokens: 100,
                cost: 0.1, // 10% of max (> 0.0)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
    ];

    calculate_intensities(&mut contributions);

    assert_eq!(contributions[0].intensity, 4); // 100%
    assert_eq!(contributions[1].intensity, 4); // 80%
    assert_eq!(contributions[2].intensity, 3); // 60%
    assert_eq!(contributions[3].intensity, 2); // 30%
    assert_eq!(contributions[4].intensity, 1); // 10%
}

#[test]
fn test_calculate_intensities_boundary_values() {
    let mut contributions = vec![
        DailyContribution {
            date: "2024-01-01".to_string(),
            totals: DailyTotals {
                tokens: 1000,
                cost: 1.0,
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
                tokens: 750,
                cost: 0.75, // Exactly 0.75 (should be level 4)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-03".to_string(),
            totals: DailyTotals {
                tokens: 500,
                cost: 0.5, // Exactly 0.5 (should be level 3)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
        DailyContribution {
            date: "2024-01-04".to_string(),
            totals: DailyTotals {
                tokens: 250,
                cost: 0.25, // Exactly 0.25 (should be level 2)
                messages: 1,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown::default(),
            clients: Vec::new(),
            active_time_ms: None,
        },
    ];

    calculate_intensities(&mut contributions);

    assert_eq!(contributions[0].intensity, 4);
    assert_eq!(contributions[1].intensity, 4); // >= 0.75
    assert_eq!(contributions[2].intensity, 3); // >= 0.5
    assert_eq!(contributions[3].intensity, 2); // >= 0.25
}
