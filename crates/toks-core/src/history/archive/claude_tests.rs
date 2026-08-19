use tempfile::tempdir;
use toks_ingest::sessions::{
    CostSource, DurableIdentity, DurableIdentityScheme, IdentityStrength, UnifiedMessage,
};
use toks_ingest::TokenBreakdown;

use super::reconcile_at;

const CAPTURED_AT: i64 = 1_776_508_800_000;

fn response(input: i64, output: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "claude".into(),
        model_id: "claude-test".into(),
        provider_id: "anthropic".into(),
        session_id: "session-a".into(),
        workspace_key: None,
        workspace_label: None,
        timestamp: 1_713_398_400_000,
        date: String::new(),
        tokens: TokenBreakdown {
            input,
            output,
            cache_read: 20,
            cache_write: 2,
            reasoning: 0,
        },
        cost: 0.0,
        cost_source: CostSource::Unknown,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some("message-1:request-1".into()),
        durable_identity: Some(DurableIdentity {
            scheme: DurableIdentityScheme::ClaudeProviderResponse,
            version: 1,
            value: "message-1:request-1".into(),
            strength: IdentityStrength::Strong,
        }),
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}

#[test]
fn later_full_response_completes_partial_accounting() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let partial = response(10, 0);
    let mut full = response(15, 5);
    full.duration_ms = Some(500);

    reconcile_at(&path, &[partial], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[full], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 15);
    assert_eq!(capture.messages[0].tokens.output, 5);
    assert_eq!(capture.messages[0].duration_ms, Some(500));
    assert_eq!(capture.conflicts, 0);
}

#[test]
fn partial_and_full_rows_in_one_scan_are_order_independent() {
    let temp = tempdir().unwrap();
    let first_path = temp.path().join("first.sqlite3");
    let second_path = temp.path().join("second.sqlite3");
    let partial = response(10, 0);
    let full = response(15, 5);

    let forward = reconcile_at(&first_path, &[partial.clone(), full.clone()], CAPTURED_AT).unwrap();
    let reverse = reconcile_at(&second_path, &[full, partial], CAPTURED_AT).unwrap();

    assert_eq!(forward.messages[0].tokens, reverse.messages[0].tokens);
    assert_eq!(forward.messages[0].tokens.input, 15);
    assert_eq!(forward.conflicts, 0);
    assert_eq!(reverse.conflicts, 0);
}

#[test]
fn later_decrease_is_quarantined() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let full = response(15, 5);
    let partial = response(10, 0);

    reconcile_at(&path, &[full], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[partial], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 15);
    assert_eq!(capture.messages[0].tokens.output, 5);
    assert_eq!(capture.conflicts, 1);
}

#[test]
fn backward_wall_clock_never_reorders_claude_contradictions() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let full = response(15, 5);
    let partial = response(10, 0);

    reconcile_at(&path, &[full], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[partial], CAPTURED_AT - 60_000).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 15);
    assert_eq!(capture.messages[0].tokens.output, 5);
    assert_eq!(capture.conflicts, 1);
}

#[test]
fn mixed_counter_change_is_quarantined() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let original = response(10, 5);
    let mixed = response(11, 4);

    reconcile_at(&path, &[original], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[mixed], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 10);
    assert_eq!(capture.messages[0].tokens.output, 5);
    assert_eq!(capture.conflicts, 1);
}

#[test]
fn provider_reported_cost_can_enrich_a_fuller_response() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let partial = response(10, 0);
    let mut full = response(15, 5);
    full.cost = 0.75;
    full.cost_source = CostSource::ProviderReported;

    reconcile_at(&path, &[partial], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[full], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 15);
    assert_eq!(capture.messages[0].cost, 0.75);
    assert_eq!(
        capture.messages[0].cost_source,
        CostSource::ProviderReported
    );
    assert_eq!(capture.conflicts, 0);
}

#[test]
fn reported_cost_cannot_disappear_during_completion() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut partial = response(10, 0);
    partial.cost = 0.75;
    partial.cost_source = CostSource::ProviderReported;
    let full_without_cost = response(15, 5);

    reconcile_at(&path, &[partial], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[full_without_cost], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 10);
    assert_eq!(capture.messages[0].cost, 0.75);
    assert_eq!(capture.conflicts, 1);
}

#[test]
fn changed_reported_cost_is_quarantined() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut partial = response(10, 0);
    partial.cost = 0.75;
    partial.cost_source = CostSource::ProviderReported;
    let mut changed = response(15, 5);
    changed.cost = 0.80;
    changed.cost_source = CostSource::ProviderReported;

    reconcile_at(&path, &[partial], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[changed], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 10);
    assert_eq!(capture.messages[0].cost, 0.75);
    assert_eq!(capture.conflicts, 1);
}
