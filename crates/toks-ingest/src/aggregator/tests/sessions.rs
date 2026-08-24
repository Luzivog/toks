use super::super::*;

#[allow(clippy::too_many_arguments)]
fn session_message(
    session_id: &str,
    client: &str,
    provider: &str,
    model: &str,
    date: &str,
    timestamp_ms: i64,
    tokens: TokenBreakdown,
    cost: f64,
) -> UnifiedMessage {
    UnifiedMessage {
        client: client.to_string(),
        model_id: model.to_string(),
        provider_id: provider.to_string(),
        session_id: session_id.to_string(),
        workspace_key: None,
        workspace_label: None,
        timestamp: timestamp_ms,
        date: date.to_string(),
        tokens,
        cost,
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

#[test]
fn test_aggregate_by_session_empty() {
    assert!(aggregate_by_session(Vec::new()).is_empty());
}

#[test]
fn test_aggregate_by_session_groups_three_sessions() {
    let t = TokenBreakdown {
        input: 100,
        output: 50,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };
    // 10 rows across 3 sessions.
    let messages = vec![
        session_message(
            "s-a",
            "codex",
            "openai",
            "gpt-5",
            "2026-05-10",
            1_700_000_001_000,
            t.clone(),
            0.01,
        ),
        session_message(
            "s-a",
            "codex",
            "openai",
            "gpt-5",
            "2026-05-10",
            1_700_000_002_000,
            t.clone(),
            0.01,
        ),
        session_message(
            "s-a",
            "codex",
            "openai",
            "gpt-5",
            "2026-05-10",
            1_700_000_003_000,
            t.clone(),
            0.01,
        ),
        session_message(
            "s-a",
            "codex",
            "openai",
            "gpt-5",
            "2026-05-10",
            1_700_000_004_000,
            t.clone(),
            0.01,
        ),
        session_message(
            "s-b",
            "amp",
            "anthropic",
            "claude-haiku-4-5",
            "2026-05-10",
            1_700_000_005_000,
            t.clone(),
            0.02,
        ),
        session_message(
            "s-b",
            "amp",
            "anthropic",
            "claude-haiku-4-5",
            "2026-05-10",
            1_700_000_006_000,
            t.clone(),
            0.02,
        ),
        session_message(
            "s-b",
            "amp",
            "anthropic",
            "claude-haiku-4-5",
            "2026-05-10",
            1_700_000_007_000,
            t.clone(),
            0.02,
        ),
        session_message(
            "s-c",
            "claude",
            "anthropic",
            "claude-sonnet-4-5",
            "2026-05-11",
            1_700_000_100_000,
            t.clone(),
            0.05,
        ),
        session_message(
            "s-c",
            "claude",
            "anthropic",
            "claude-sonnet-4-5",
            "2026-05-11",
            1_700_000_101_000,
            t.clone(),
            0.05,
        ),
        session_message(
            "s-c",
            "claude",
            "anthropic",
            "claude-sonnet-4-5",
            "2026-05-11",
            1_700_000_102_000,
            t.clone(),
            0.05,
        ),
    ];

    let result = aggregate_by_session(messages);
    assert_eq!(result.len(), 3, "expected 3 sessions");

    // Most-recent-first ordering: s-c last_seen=1_700_000_102 wins.
    assert_eq!(result[0].session_id, "s-c");
    assert_eq!(result[1].session_id, "s-b");
    assert_eq!(result[2].session_id, "s-a");

    let s_a = result.iter().find(|s| s.session_id == "s-a").unwrap();
    assert_eq!(s_a.totals.messages, 4);
    assert_eq!(s_a.totals.tokens, 4 * 150); // (100 input + 50 output) * 4
    assert!((s_a.totals.cost - 0.04).abs() < 1e-9);
    assert_eq!(s_a.token_breakdown.input, 400);
    assert_eq!(s_a.token_breakdown.output, 200);
    assert_eq!(s_a.client, "codex");
    assert_eq!(s_a.provider, "openai");
    assert_eq!(s_a.model, "gpt-5");
    // Timestamps converted to seconds.
    assert_eq!(s_a.first_seen, 1_700_000_001);
    assert_eq!(s_a.last_seen, 1_700_000_004);

    let s_b = result.iter().find(|s| s.session_id == "s-b").unwrap();
    assert_eq!(s_b.totals.messages, 3);
    assert!((s_b.totals.cost - 0.06).abs() < 1e-9);

    let s_c = result.iter().find(|s| s.session_id == "s-c").unwrap();
    assert_eq!(s_c.totals.messages, 3);
    assert!((s_c.totals.cost - 0.15).abs() < 1e-9);
}

#[test]
fn test_aggregate_by_session_picks_top_client_by_cost() {
    // Same session_id but two different clients — top-level fields should
    // reflect the client with the larger cost share.
    let small = TokenBreakdown {
        input: 10,
        output: 10,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };
    let big = TokenBreakdown {
        input: 1000,
        output: 500,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };
    let messages = vec![
        session_message(
            "shared",
            "amp",
            "anthropic",
            "claude-haiku-4-5",
            "2026-05-10",
            1_700_000_001_000,
            small,
            0.001,
        ),
        session_message(
            "shared",
            "codex",
            "openai",
            "gpt-5",
            "2026-05-10",
            1_700_000_002_000,
            big,
            0.50,
        ),
    ];

    let result = aggregate_by_session(messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].client, "codex");
    assert_eq!(result[0].provider, "openai");
    assert_eq!(result[0].model, "gpt-5");
    // Per-client breakdown should preserve both clients.
    assert_eq!(result[0].clients.len(), 2);
    assert_eq!(result[0].clients[0].client, "codex");
    assert!((result[0].totals.cost - 0.501).abs() < 1e-9);
}

#[test]
fn test_session_contribution_serde_round_trip() {
    let contrib = SessionContribution {
        session_id: "019e1e27-af49-7cd1-89b7-7bad1c3f3be2".to_string(),
        client: "codex".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5".to_string(),
        totals: DailyTotals {
            tokens: 25298,
            cost: 0.0123,
            messages: 12,
        },
        token_breakdown: TokenBreakdown {
            input: 25_251,
            output: 47,
            cache_read: 1_920,
            cache_write: 0,
            reasoning: 40,
        },
        clients: vec![ClientContribution {
            client: "codex".to_string(),
            model_id: "gpt-5".to_string(),
            provider_id: "openai".to_string(),
            tokens: TokenBreakdown {
                input: 25_251,
                output: 47,
                cache_read: 1_920,
                cache_write: 0,
                reasoning: 40,
            },
            cost: 0.0123,
            messages: 12,
        }],
        first_seen: 1_715_551_577,
        last_seen: 1_715_551_612,
    };

    let json = serde_json::to_string(&contrib).expect("serialize");
    let parsed: SessionContribution = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, contrib);
    // Spot-check key field is present in JSON.
    assert!(json.contains("\"session_id\":\"019e1e27"));
}
