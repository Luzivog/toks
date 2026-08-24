use super::super::*;
use super::mock_unified_message;

#[test]
fn test_aggregate_by_date_empty() {
    let messages = Vec::new();
    let result = aggregate_by_date(messages);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_aggregate_by_date_single_message() {
    let messages = vec![mock_unified_message(
        "2024-01-01",
        1000,
        0.05,
        "claude-3-5-sonnet",
        "opencode",
    )];

    let result = aggregate_by_date(messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].date, "2024-01-01");
    assert_eq!(result[0].totals.tokens, 1000);
    assert_eq!(result[0].totals.cost, 0.05);
    assert_eq!(result[0].totals.messages, 1);
}

#[test]
fn test_aggregate_by_date_multiple_dates() {
    let messages = vec![
        mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-01-02", 2000, 0.10, "gpt-4", "claude"),
        mock_unified_message("2024-01-03", 1500, 0.08, "claude-3-5-sonnet", "opencode"),
    ];

    let result = aggregate_by_date(messages);
    assert_eq!(result.len(), 3);

    // Verify sorted by date
    assert_eq!(result[0].date, "2024-01-01");
    assert_eq!(result[1].date, "2024-01-02");
    assert_eq!(result[2].date, "2024-01-03");

    // Verify totals
    assert_eq!(result[0].totals.tokens, 1000);
    assert_eq!(result[1].totals.tokens, 2000);
    assert_eq!(result[2].totals.tokens, 1500);
}

#[test]
fn test_aggregate_by_date_same_date_aggregation() {
    let messages = vec![
        mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-01-01", 2000, 0.10, "gpt-4", "claude"),
        mock_unified_message("2024-01-01", 1500, 0.08, "claude-3-5-sonnet", "opencode"),
    ];

    let result = aggregate_by_date(messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].date, "2024-01-01");
    assert_eq!(result[0].totals.tokens, 4500);
    assert!((result[0].totals.cost - 0.23).abs() < 0.0001);
    assert_eq!(result[0].totals.messages, 3);
}

#[test]
fn test_aggregate_by_date_token_breakdown() {
    let mut msg = mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode");
    msg.tokens = TokenBreakdown {
        input: 600,
        output: 300,
        cache_read: 50,
        cache_write: 40,
        reasoning: 10,
    };

    let result = aggregate_by_date(vec![msg]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].token_breakdown.input, 600);
    assert_eq!(result[0].token_breakdown.output, 300);
    assert_eq!(result[0].token_breakdown.cache_read, 50);
    assert_eq!(result[0].token_breakdown.cache_write, 40);
    assert_eq!(result[0].token_breakdown.reasoning, 10);
}

#[test]
fn test_aggregate_by_date_preserves_sources() {
    let messages = vec![
        mock_unified_message("2024-01-01", 1000, 0.05, "claude-3-5-sonnet", "opencode"),
        mock_unified_message("2024-01-01", 2000, 0.10, "gpt-4", "claude"),
    ];

    let result = aggregate_by_date(messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].clients.len(), 2);

    // Verify both clients are present
    let client_names: Vec<&str> = result[0]
        .clients
        .iter()
        .map(|s| s.client.as_str())
        .collect();
    assert!(client_names.contains(&"opencode"));
    assert!(client_names.contains(&"claude"));
}

#[test]
fn test_aggregate_by_date_large_dataset() {
    // Test with 100 messages across 10 days
    let mut messages = Vec::new();
    for day in 1..=10 {
        for _msg in 0..10 {
            let date = format!("2024-01-{:02}", day);
            messages.push(mock_unified_message(
                &date,
                1000,
                0.05,
                "claude-3-5-sonnet",
                "opencode",
            ));
        }
    }

    let result = aggregate_by_date(messages);
    assert_eq!(result.len(), 10);

    // Each day should have 10 messages aggregated
    for contribution in &result {
        assert_eq!(contribution.totals.messages, 10);
        assert_eq!(contribution.totals.tokens, 10000);
        assert!((contribution.totals.cost - 0.5).abs() < 0.0001);
    }
}
