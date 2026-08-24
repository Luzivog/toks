use super::*;
#[test]
fn monthly_reasoning_matches_model_and_hourly_aggregation() {
    let messages = vec![
        UnifiedMessage::new(
            "opencode",
            "reasoning-model",
            "openai",
            "session-a",
            1_767_225_600_000,
            TokenBreakdown {
                input: 10,
                output: 5,
                cache_read: 2,
                cache_write: 1,
                reasoning: 7,
            },
            0.1,
        ),
        UnifiedMessage::new(
            "codex",
            "reasoning-model",
            "openai",
            "session-b",
            1_767_229_200_000,
            TokenBreakdown {
                input: 20,
                output: 8,
                cache_read: 3,
                cache_write: 2,
                reasoning: 11,
            },
            0.2,
        ),
    ];

    let monthly = aggregate_monthly_usage_v2_entries(messages.clone());
    let models = aggregate_model_usage_entries(messages.clone(), &GroupBy::Model);
    let hourly = aggregate_hourly_usage_entries(
        messages,
        crate::bucket_tz::BucketTimezone::from_pinned_name(Some("UTC")),
    );

    assert_eq!(monthly.len(), 1);
    let monthly_reasoning = monthly[0].reasoning;
    let model_reasoning = models
        .iter()
        .fold(0_i64, |total, entry| total.saturating_add(entry.reasoning));
    let hourly_reasoning = hourly
        .iter()
        .fold(0_i64, |total, entry| total.saturating_add(entry.reasoning));
    assert_eq!(monthly_reasoning, 18);
    assert_eq!(monthly_reasoning, model_reasoning);
    assert_eq!(monthly_reasoning, hourly_reasoning);
}

#[test]
fn monthly_aggregation_rejects_malformed_calendar_dates() {
    let message_with_date = |date: &str, input: i64| {
        let mut message = UnifiedMessage::new(
            "codex",
            "model",
            "openai",
            format!("session-{input}"),
            1_767_225_600_000,
            TokenBreakdown {
                input,
                ..TokenBreakdown::default()
            },
            0.0,
        );
        message.date = date.to_string();
        message
    };

    let monthly = aggregate_monthly_usage_v2_entries([
        message_with_date("2024-02-29", 1),
        message_with_date("2023-02-29", 2),
        message_with_date("2026-00-01", 4),
        message_with_date("2026-13-01", 8),
        message_with_date("2026-04-31", 16),
        message_with_date("2026-💥", 32),
        message_with_date("2026-01-31", 64),
    ]);

    assert_eq!(monthly.len(), 2);
    assert_eq!(
        monthly
            .iter()
            .find(|entry| entry.month == "2024-02")
            .unwrap()
            .input,
        1
    );
    assert_eq!(
        monthly
            .iter()
            .find(|entry| entry.month == "2026-01")
            .unwrap()
            .input,
        64
    );
}

#[test]
fn monthly_message_count_saturates() {
    let mut first = UnifiedMessage::new(
        "codex",
        "model",
        "openai",
        "session-a",
        1_767_225_600_000,
        TokenBreakdown::default(),
        0.0,
    );
    first.message_count = i32::MAX;
    let second = UnifiedMessage::new(
        "codex",
        "model",
        "openai",
        "session-b",
        1_767_225_601_000,
        TokenBreakdown::default(),
        0.0,
    );

    let monthly = aggregate_monthly_usage_v2_entries([first, second]);
    assert_eq!(monthly[0].message_count, i32::MAX);
}

#[test]
fn model_aggregation_saturates_overflowing_token_folds() {
    // token_total_saturates_on_overlarge_buckets covers a single message's
    // grand total; the per-field CROSS-MESSAGE fold in
    // aggregate_model_usage_entries must saturate too. An antigravity-cli
    // row can carry an i64::MAX bucket after the untrusted-varint clamp
    // (sessions/antigravity_cli.rs to_i64), so two such rows folded into one
    // model group with plain `+=` overflow (debug panic / release wrap)
    // before the already-saturating grand total runs.
    let make = || {
        UnifiedMessage::new_with_dedup(
            "antigravity-cli",
            "gemini-3-pro",
            "antigravity",
            "session-overflow",
            1_733_011_200_000,
            TokenBreakdown {
                input: i64::MAX,
                output: 0,
                cache_read: i64::MAX,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
            None,
        )
    };

    let entries = aggregate_model_usage_entries(vec![make(), make()], &GroupBy::Model);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].input, i64::MAX);
    assert_eq!(entries[0].cache_read, i64::MAX);
}

#[test]
fn model_report_totals_saturate_across_groups() {
    // aggregate_model_usage_entries saturates each entry's fields, so an
    // entry can be i64::MAX. get_model_report sums the entries into the
    // report-level totals via model_report_token_totals; two saturated
    // entries (two distinct models) must not overflow that sum either.
    let make = |model: &str| {
        UnifiedMessage::new_with_dedup(
            "antigravity-cli",
            model,
            "antigravity",
            "session-overflow",
            1_733_011_200_000,
            TokenBreakdown {
                input: i64::MAX,
                output: 0,
                cache_read: i64::MAX,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
            None,
        )
    };

    let entries = aggregate_model_usage_entries(
        vec![make("gemini-3-pro"), make("claude-opus-4-6")],
        &GroupBy::Model,
    );
    assert_eq!(entries.len(), 2);
    let (total_input, _total_output, total_cache_read, _total_cache_write) =
        crate::model_report_token_totals(&entries);
    assert_eq!(total_input, i64::MAX);
    assert_eq!(total_cache_read, i64::MAX);
}
