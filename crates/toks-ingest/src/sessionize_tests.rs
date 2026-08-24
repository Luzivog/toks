use super::*;
use chrono::{FixedOffset, TimeZone};
fn make_msg(client: &str, session_id: &str, timestamp: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: client.to_string(),
        model_id: "test-model".to_string(),
        provider_id: "test-provider".to_string(),
        session_id: session_id.to_string(),
        workspace_key: None,
        workspace_label: None,
        timestamp,
        date: "2024-01-01".to_string(),
        tokens: TokenBreakdown {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        cost: 0.01,
        cost_source: Default::default(),
        message_count: 1,
        agent: None,
        dedup_key: None,
        durable_identity: None,
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: false,
        model_attribution_conflicted: false,
        duration_ms: None,
    }
}
fn make_timed_msg(
    client: &str,
    session_id: &str,
    timestamp: i64,
    duration_ms: i64,
) -> UnifiedMessage {
    let mut message = make_msg(client, session_id, timestamp);
    message.duration_ms = Some(duration_ms);
    message
}

#[test]
fn test_sessionize_empty() {
    let result = sessionize(&[], DEFAULT_IDLE_GAP_MS);
    assert!(result.is_empty());
}

#[test]
fn test_sessionize_single_message() {
    let msgs = vec![make_msg("opencode", "ses1", 1000000)];
    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].wall_duration_ms, 0);
    assert_eq!(result[0].active_duration_ms, 0);
    assert_eq!(result[0].message_count, 1);
}

#[test]
fn test_sessionize_saturates_overflowing_token_fold() {
    // A parser can clamp an untrusted token count to i64::MAX (e.g.
    // antigravity_cli). Two such messages in one (client, session_id) block
    // within the idle gap fold into the same SessionBlock; a plain `+=`
    // fold would overflow (debug panic / release wrap) before it is
    // serialized into SessionInterval.tokens.
    let overlarge = |ts: i64| {
        let mut message = make_msg("antigravity-cli", "ses1", ts);
        message.tokens = TokenBreakdown {
            input: i64::MAX,
            output: 0,
            cache_read: i64::MAX,
            cache_write: 0,
            reasoning: 0,
        };
        message
    };
    let msgs = vec![overlarge(1_000_000), overlarge(1_001_000)];
    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].tokens.input, i64::MAX);
    assert_eq!(result[0].tokens.cache_read, i64::MAX);
}

#[test]
fn test_sessionize_counts_single_timed_message_duration() {
    let msgs = vec![make_timed_msg("opencode", "ses1", 1_000_000, 45_000)];
    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].start_ts, 1_000_000);
    assert_eq!(result[0].end_ts, 1_045_000);
    assert_eq!(result[0].wall_duration_ms, 45_000);
    assert_eq!(result[0].active_duration_ms, 45_000);
}

#[test]
fn test_sessionize_splits_timed_messages_across_idle_gap() {
    let msgs = vec![
        make_timed_msg("opencode", "ses1", 1_000_000, 60_000),
        make_timed_msg("opencode", "ses1", 1_000_000 + 10 * 60_000, 30_000),
    ];

    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].start_ts, 1_000_000);
    assert_eq!(result[0].end_ts, 1_060_000);
    assert_eq!(result[0].active_duration_ms, 60_000);
    assert_eq!(result[1].start_ts, 1_600_000);
    assert_eq!(result[1].end_ts, 1_630_000);
    assert_eq!(result[1].active_duration_ms, 30_000);
}

#[test]
fn test_sessionize_continuous_session() {
    // 5 messages, each 1 minute apart (within 3-min threshold)
    let msgs: Vec<UnifiedMessage> = (0..5)
        .map(|i| make_msg("opencode", "ses1", 1000000 + i * 60_000))
        .collect();

    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].wall_duration_ms, 4 * 60_000);
    assert_eq!(result[0].active_duration_ms, 4 * 60_000); // all gaps <= 3min
    assert_eq!(result[0].message_count, 5);
}

#[test]
fn test_sessionize_with_idle_gap() {
    // 3 messages: first two 1 min apart, then 5 min gap (exceeds 3-min threshold)
    let msgs = vec![
        make_msg("opencode", "ses1", 1000000),
        make_msg("opencode", "ses1", 1000000 + 60_000),
        make_msg("opencode", "ses1", 1000000 + 60_000 + 5 * 60_000),
    ];

    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].active_duration_ms, 60_000);
    assert_eq!(result[0].wall_duration_ms, 60_000);
    assert_eq!(result[0].message_count, 2);
    assert_eq!(result[1].active_duration_ms, 0);
    assert_eq!(result[1].wall_duration_ms, 0);
    assert_eq!(result[1].message_count, 1);
}

#[test]
fn test_sessionize_preserves_count_neutral_split_fragments() {
    let mut counted = make_msg("copilot", "session-1", 1_000_000);
    counted.message_count = 1;
    let mut split_model = make_msg("copilot", "session-1", 1_000_000 + DEFAULT_IDLE_GAP_MS + 1);
    split_model.message_count = 0;
    let mut residual = make_msg(
        "copilot",
        "session-1",
        1_000_000 + 2 * (DEFAULT_IDLE_GAP_MS + 1),
    );
    residual.message_count = 0;

    let result = sessionize(&[counted, split_model, residual], DEFAULT_IDLE_GAP_MS);

    assert_eq!(
        result.len(),
        3,
        "idle-separated activity remains three time blocks"
    );
    assert_eq!(
        result
            .iter()
            .map(|interval| interval.message_count)
            .sum::<i32>(),
        1
    );
    let metrics = compute_time_metrics(&result, DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.session_count, 1);
}

#[test]
fn test_sessionize_multiple_sessions() {
    let msgs = vec![
        make_msg("opencode", "ses1", 1000000),
        make_msg("opencode", "ses1", 1060000),
        make_msg("claude", "ses2", 1000000),
        make_msg("claude", "ses2", 1120000),
    ];

    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_sessionize_skips_zero_timestamp() {
    let msgs = vec![
        make_msg("opencode", "ses1", 0),
        make_msg("opencode", "ses1", 1000000),
    ];

    let result = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message_count, 1); // only the non-zero one
}

#[test]
fn test_compute_time_metrics_empty() {
    let metrics = compute_time_metrics(&[], DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.total_active_time_ms, 0);
    assert_eq!(metrics.longest_continuous_ms, 0);
    assert_eq!(metrics.max_concurrent_sessions, 0);
    assert_eq!(metrics.session_count, 0);
}

#[test]
fn test_max_concurrent_sessions() {
    // Two overlapping sessions
    let intervals = vec![
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses1".to_string(),
            start_ts: 1000,
            end_ts: 5000,
            wall_duration_ms: 4000,
            active_duration_ms: 4000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
        SessionInterval {
            client: "claude".to_string(),
            session_id: "ses2".to_string(),
            start_ts: 3000,
            end_ts: 7000,
            wall_duration_ms: 4000,
            active_duration_ms: 4000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
    ];

    let metrics = compute_time_metrics(&intervals, DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.max_concurrent_sessions, 2);
}

#[test]
fn test_max_concurrent_non_overlapping() {
    // Two non-overlapping sessions
    let intervals = vec![
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses1".to_string(),
            start_ts: 1000,
            end_ts: 3000,
            wall_duration_ms: 2000,
            active_duration_ms: 2000,
            message_count: 2,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
        SessionInterval {
            client: "claude".to_string(),
            session_id: "ses2".to_string(),
            start_ts: 5000,
            end_ts: 7000,
            wall_duration_ms: 2000,
            active_duration_ms: 2000,
            message_count: 2,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
    ];

    let metrics = compute_time_metrics(&intervals, DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.max_concurrent_sessions, 1);
}

#[test]
fn test_longest_continuous_is_max_session_active_duration() {
    let intervals = vec![
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses1".to_string(),
            start_ts: 1000,
            end_ts: 5000,
            wall_duration_ms: 4000,
            active_duration_ms: 3000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
        SessionInterval {
            client: "claude".to_string(),
            session_id: "ses2".to_string(),
            start_ts: 3000,
            end_ts: 8000,
            wall_duration_ms: 5000,
            active_duration_ms: 5000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
    ];

    let metrics = compute_time_metrics(&intervals, DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.longest_continuous_ms, 7000);
}

#[test]
fn test_longest_continuous_picks_max_active() {
    let intervals = vec![
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses1".to_string(),
            start_ts: 1,
            end_ts: 60_000,
            wall_duration_ms: 60_000,
            active_duration_ms: 60_000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses2".to_string(),
            start_ts: 60_000 + 2 * 60_000,
            end_ts: 60_000 + 2 * 60_000 + 120_000,
            wall_duration_ms: 120_000,
            active_duration_ms: 120_000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
    ];

    let metrics = compute_time_metrics(&intervals, DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.longest_continuous_ms, 299_999);
}

#[test]
fn test_longest_continuous_single_session() {
    let intervals = vec![
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses1".to_string(),
            start_ts: 1000,
            end_ts: 61_000,
            wall_duration_ms: 60_000,
            active_duration_ms: 60_000,
            message_count: 3,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
        SessionInterval {
            client: "opencode".to_string(),
            session_id: "ses2".to_string(),
            start_ts: 61_000 + 10 * 60_000,
            end_ts: 61_000 + 10 * 60_000 + 120_000,
            wall_duration_ms: 120_000,
            active_duration_ms: 120_000,
            message_count: 5,
            tokens: TokenBreakdown::default(),
            cost: 0.0,
        },
    ];

    let metrics = compute_time_metrics(&intervals, DEFAULT_IDLE_GAP_MS);
    assert_eq!(metrics.longest_continuous_ms, 120_000);
}

#[test]
fn test_compute_daily_active_time_matches_local_day_boundaries_for_fixed_offset() {
    let interval = SessionInterval {
        client: "trae".to_string(),
        session_id: "session-local-boundary".to_string(),
        start_ts: FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 1, 23, 30, 0)
            .single()
            .unwrap()
            .timestamp_millis(),
        end_ts: FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 2, 0, 30, 0)
            .single()
            .unwrap()
            .timestamp_millis(),
        wall_duration_ms: 3_600_000,
        active_duration_ms: 3_600_000,
        message_count: 2,
        tokens: TokenBreakdown::default(),
        cost: 0.0,
    };

    let daily = compute_daily_active_time_with_timezone(
        &[interval],
        &FixedOffset::east_opt(9 * 3600).unwrap(),
    );

    assert_eq!(daily.get("2026-01-01"), Some(&1_800_000));
    assert_eq!(daily.get("2026-01-02"), Some(&1_800_000));
    assert_eq!(daily.len(), 2);
}

#[test]
fn test_daily_active_time_keeps_idle_separated_timed_blocks_on_their_days() {
    let timezone = FixedOffset::east_opt(0).unwrap();
    let first_start = timezone
        .with_ymd_and_hms(2026, 1, 1, 23, 58, 0)
        .single()
        .unwrap()
        .timestamp_millis();
    let second_start = timezone
        .with_ymd_and_hms(2026, 1, 2, 0, 10, 0)
        .single()
        .unwrap()
        .timestamp_millis();
    let msgs = vec![
        make_timed_msg("opencode", "ses1", first_start, 120_000),
        make_timed_msg("opencode", "ses1", second_start, 60_000),
    ];

    let intervals = sessionize(&msgs, DEFAULT_IDLE_GAP_MS);
    let daily = compute_daily_active_time_with_timezone(&intervals, &timezone);

    assert_eq!(intervals.len(), 2);
    assert_eq!(daily.get("2026-01-01"), Some(&120_000));
    assert_eq!(daily.get("2026-01-02"), Some(&60_000));
    assert_eq!(daily.len(), 2);
}
