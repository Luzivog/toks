use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use tempfile::tempdir;
use tokscope_ingest::sessions::{
    CostSource, DurableIdentity, DurableIdentityScheme, IdentityStrength, UnifiedMessage,
};
use tokscope_ingest::TokenBreakdown;

use super::{load_at, reconcile_at, schema};

const OBSERVED_AT: i64 = 1_776_508_800_000;

fn message(key: Option<&str>, session: &str, timestamp: i64, input: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        model_id: "gpt-test".into(),
        provider_id: "openai".into(),
        session_id: session.into(),
        workspace_key: None,
        workspace_label: None,
        timestamp,
        date: "2026-04-18".into(),
        tokens: TokenBreakdown {
            input,
            output: 5,
            cache_read: 20,
            cache_write: 2,
            reasoning: 3,
        },
        cost: 1.25,
        cost_source: CostSource::Estimated,
        duration_ms: Some(200),
        message_count: 2,
        agent: None,
        dedup_key: key.map(str::to_owned),
        durable_identity: key.map(|value| DurableIdentity {
            scheme: DurableIdentityScheme::CodexSessionTimestampOccurrence,
            version: 1,
            value: value.into(),
            strength: IdentityStrength::SessionStable,
        }),
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}

#[test]
fn missing_logs_never_delete_accepted_usage() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let original = message(Some("request-1"), "session-a", 1_713_398_400_000, 10);

    let first = reconcile_at(&path, &[original], OBSERVED_AT).unwrap();
    let after_missing_scan = reconcile_at(&path, &[], OBSERVED_AT + 1).unwrap();

    assert_eq!(first.messages.len(), 1);
    assert_eq!(after_missing_scan.messages.len(), 1);
    assert_eq!(after_missing_scan.messages[0].tokens.input, 10);
}

#[test]
fn empty_first_reconcile_does_not_initialize_history() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");

    let capture = reconcile_at(&path, &[], OBSERVED_AT).unwrap();

    assert!(capture.messages.is_empty());
    assert!(load_at(&path).unwrap().is_none());
}

#[test]
fn strong_and_weak_replays_are_idempotent() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let strong = message(Some("request-1"), "session-a", 1_713_398_400_000, 10);
    let weak = message(None, "session-b", 1_713_402_000_000, 11);

    reconcile_at(&path, &[strong.clone(), weak.clone()], OBSERVED_AT).unwrap();
    let capture = reconcile_at(
        &path,
        &[strong.clone(), weak.clone(), strong, weak],
        OBSERVED_AT + 1,
    )
    .unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(capture.strong_events, 0);
    assert_eq!(capture.weak_events, 2);
    assert_eq!(capture.conflicts, 0);
}

#[test]
fn identical_weak_facts_in_different_sessions_remain_distinct() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = message(None, "session-a", 1_713_398_400_000, 10);
    let second = message(None, "session-b", 1_713_398_400_000, 10);

    let capture = reconcile_at(&path, &[first, second], OBSERVED_AT).unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(capture.weak_events, 2);
}

#[test]
fn claude_path_scoped_tool_results_are_weak() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut first = message(
        Some("claude:tool_result:session-a:tool-1"),
        "session-a",
        1_713_398_400_000,
        10,
    );
    first.client = "claude".into();
    first.durable_identity = None;
    let mut second = first.clone();
    second.session_id = "session-b".into();
    second.dedup_key = Some("claude:tool_result:session-b:tool-1".into());

    let capture = reconcile_at(&path, &[first, second], OBSERVED_AT).unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(capture.weak_events, 2);
    assert_eq!(capture.strong_events, 0);
}

#[test]
fn copied_session_stable_event_does_not_double_count() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = message(Some("request-1"), "original-session", 1_713_398_400_000, 10);
    let mut copy = first.clone();
    copy.session_id = "copied-session".into();

    let capture = reconcile_at(&path, &[first, copy], OBSERVED_AT).unwrap();

    assert_eq!(capture.messages.len(), 1);
    let connection = schema::open(&path).unwrap();
    let sources: i64 = connection
        .query_row("SELECT COUNT(*) FROM event_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sources, 2);
}

#[test]
fn session_identity_upgrades_one_matching_fallback_event() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let weak = message(None, "session", 1_713_398_400_000, 10);
    let mut session_stable = weak.clone();
    session_stable.dedup_key = Some("request-1".into());
    session_stable.durable_identity = Some(DurableIdentity {
        scheme: DurableIdentityScheme::CodexSessionTimestampOccurrence,
        version: 1,
        value: "request-1".into(),
        strength: IdentityStrength::SessionStable,
    });

    reconcile_at(&path, &[weak], OBSERVED_AT).unwrap();
    let capture = reconcile_at(&path, &[session_stable], OBSERVED_AT + 1).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.strong_events, 0);
    assert_eq!(capture.weak_events, 1);
    let connection = schema::open(&path).unwrap();
    let identities: i64 = connection
        .query_row("SELECT COUNT(*) FROM identities", [], |row| row.get(0))
        .unwrap();
    assert_eq!(identities, 2);
}

#[test]
fn conflicts_choose_the_same_canonical_fact_in_any_order() {
    let temp = tempdir().unwrap();
    let first_path = temp.path().join("first.sqlite3");
    let second_path = temp.path().join("second.sqlite3");
    let smaller = message(Some("request-1"), "session", 1_713_398_400_000, 10);
    let larger = message(Some("request-1"), "session", 1_713_398_400_000, 50);

    let forward =
        reconcile_at(&first_path, &[smaller.clone(), larger.clone()], OBSERVED_AT).unwrap();
    let reversed = reconcile_at(&second_path, &[larger, smaller], OBSERVED_AT).unwrap();

    assert_eq!(forward.messages[0].tokens, reversed.messages[0].tokens);
    assert_eq!(forward.conflicts, 1);
    assert_eq!(reversed.conflicts, 1);
}

#[test]
fn conflicting_revisions_remain_bounded() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = (1..=20)
        .map(|input| message(Some("request-1"), "session", 1_713_398_400_000, input))
        .collect::<Vec<_>>();

    let capture = reconcile_at(&path, &observations, OBSERVED_AT).unwrap();
    let connection = schema::open(&path).unwrap();
    let revisions: i64 = connection
        .query_row("SELECT COUNT(*) FROM event_revisions", [], |row| row.get(0))
        .unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.conflicts, 1);
    assert_eq!(revisions, 4);
}

#[test]
fn rejected_source_does_not_rollback_a_prior_committed_source() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let valid = message(Some("valid"), "session-a", 1_713_398_400_000, 10);
    let invalid = message(Some("invalid"), "session-b", 1_713_402_000_000, -1);

    assert!(reconcile_at(&path, &[valid, invalid], OBSERVED_AT).is_err());
    let capture = load_at(&path).unwrap().unwrap();
    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.messages[0].tokens.input, 10);
}

#[test]
fn archive_does_not_persist_conversation_metadata() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut sensitive = message(
        Some("secret-dedup-marker"),
        "secret-session-marker",
        1_713_398_400_000,
        10,
    );
    sensitive.workspace_key = Some("secret-workspace-key".into());
    sensitive.workspace_label = Some("secret-workspace-label".into());
    sensitive.session_title = Some("secret-conversation-title".into());
    sensitive.agent = Some("secret-agent-name".into());
    sensitive
        .accounting_aliases
        .push(tokscope_ingest::AccountingAlias {
            scheme: tokscope_ingest::AccountingAliasScheme::CodexForkReplayDedup,
            version: 1,
            value: "secret-accounting-alias".into(),
        });

    let capture = reconcile_at(&path, &[sensitive], OBSERVED_AT).unwrap();
    let loaded = &capture.messages[0];
    assert!(loaded.workspace_key.is_none());
    assert!(loaded.workspace_label.is_none());
    assert!(loaded.session_title.is_none());
    assert!(loaded.agent.is_none());
    assert!(loaded.session_id.starts_with("archive:"));

    let mut bytes = Vec::new();
    for entry in fs::read_dir(temp.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            bytes.extend(fs::read(entry.path()).unwrap());
        }
    }
    let stored = String::from_utf8_lossy(&bytes);
    for secret in [
        "secret-dedup-marker",
        "secret-session-marker",
        "secret-workspace-key",
        "secret-workspace-label",
        "secret-conversation-title",
        "secret-agent-name",
        "secret-accounting-alias",
    ] {
        assert!(!stored.contains(secret), "archive leaked {secret}");
    }
}

#[test]
fn concurrent_writers_serialize_without_losing_events() {
    let temp = tempdir().unwrap();
    let path = Arc::new(temp.path().join("usage.sqlite3"));
    let barrier = Arc::new(Barrier::new(4));
    let handles = (0..4)
        .map(|index| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let event = message(
                    Some(&format!("request-{index}")),
                    &format!("session-{index}"),
                    1_713_398_400_000 + index * 1_000,
                    10 + index,
                );
                barrier.wait();
                reconcile_at(&path, &[event], OBSERVED_AT + index).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }

    let capture = load_at(&path).unwrap().unwrap();
    assert_eq!(capture.messages.len(), 4);
    assert_eq!(capture.strong_events, 0);
    assert_eq!(capture.weak_events, 4);
}

#[test]
fn capture_bounds_and_cost_projection_survive_reload() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = message(Some("first"), "session-a", 1_713_398_400_000, 10);
    let mut last = message(Some("last"), "session-b", 1_713_405_600_000, 20);
    last.cost = 4.0;
    last.cost_source = CostSource::ProviderReported;

    reconcile_at(&path, &[first, last], OBSERVED_AT).unwrap();
    let capture = load_at(&path).unwrap().unwrap();

    assert_eq!(capture.captured_since_ms, Some(OBSERVED_AT));
    assert_eq!(capture.captured_through_ms, Some(OBSERVED_AT));
    assert_eq!(capture.messages[1].cost, 4.0);
    assert_eq!(
        capture.messages[1].cost_source,
        CostSource::ProviderReported
    );
}

#[test]
fn later_token_contradictions_never_replace_accepted_facts() {
    let temp = tempdir().unwrap();
    let smaller_first_path = temp.path().join("smaller.sqlite3");
    let larger_first_path = temp.path().join("larger.sqlite3");
    let smaller = message(Some("request"), "session", 1_713_398_400_000, 10);
    let larger = message(Some("request"), "session", 1_713_398_400_000, 50);

    reconcile_at(
        &smaller_first_path,
        std::slice::from_ref(&smaller),
        OBSERVED_AT,
    )
    .unwrap();
    let smaller_first = reconcile_at(
        &smaller_first_path,
        std::slice::from_ref(&larger),
        OBSERVED_AT + 1,
    )
    .unwrap();
    reconcile_at(&larger_first_path, &[larger], OBSERVED_AT).unwrap();
    let larger_first = reconcile_at(&larger_first_path, &[smaller], OBSERVED_AT + 1).unwrap();

    assert_eq!(smaller_first.messages[0].tokens.input, 10);
    assert_eq!(larger_first.messages[0].tokens.input, 50);
    assert_eq!(smaller_first.conflicts, 1);
    assert_eq!(larger_first.conflicts, 1);
}

#[test]
fn backward_wall_clock_never_reorders_accepted_codex_facts() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let accepted = message(Some("request"), "session", 1_713_398_400_000, 10);
    let contradiction = message(Some("request"), "session", 1_713_398_400_000, 50);

    reconcile_at(&path, &[accepted], OBSERVED_AT).unwrap();
    let capture = reconcile_at(&path, &[contradiction], OBSERVED_AT - 60_000).unwrap();

    assert_eq!(capture.messages[0].tokens.input, 10);
    assert_eq!(capture.conflicts, 1);
}

#[test]
fn capture_provenance_advances_after_an_empty_scan() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let event = message(Some("first"), "session", 1_600_000_000_000, 10);

    reconcile_at(&path, &[event], OBSERVED_AT).unwrap();
    let capture = reconcile_at(&path, &[], OBSERVED_AT + 60_000).unwrap();

    assert_eq!(capture.captured_since_ms, Some(OBSERVED_AT));
    assert_eq!(capture.captured_through_ms, Some(OBSERVED_AT + 60_000));
}

#[test]
fn newer_schema_is_refused_without_mutation() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 5).unwrap();
    drop(connection);

    assert!(schema::open(&path).is_err());
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 5);
}

#[test]
fn provider_reported_cost_outranks_an_estimate_in_any_order() {
    let temp = tempdir().unwrap();
    let first_path = temp.path().join("first.sqlite3");
    let second_path = temp.path().join("second.sqlite3");
    let estimated = message(Some("request"), "session", 1_713_398_400_000, 10);
    let mut reported = estimated.clone();
    reported.cost = 0.75;
    reported.cost_source = CostSource::ProviderReported;

    let forward = reconcile_at(
        &first_path,
        &[estimated.clone(), reported.clone()],
        OBSERVED_AT,
    )
    .unwrap();
    let reverse = reconcile_at(&second_path, &[reported, estimated], OBSERVED_AT).unwrap();

    assert_eq!(forward.messages[0].cost, 0.75);
    assert_eq!(reverse.messages[0].cost, 0.75);
    assert_eq!(
        forward.messages[0].cost_source,
        CostSource::ProviderReported
    );
    assert_eq!(
        reverse.messages[0].cost_source,
        CostSource::ProviderReported
    );
}

#[test]
fn reported_cost_enriches_a_weak_event_without_duplication() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let estimate = message(None, "session", 1_713_398_400_000, 10);
    let mut reported = estimate.clone();
    reported.cost = 0.75;
    reported.cost_source = CostSource::ProviderReported;

    reconcile_at(&path, &[estimate], OBSERVED_AT).unwrap();
    let capture = reconcile_at(&path, &[reported], OBSERVED_AT + 1).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.messages[0].cost, 0.75);
    assert_eq!(capture.weak_events, 1);
}

#[test]
fn estimated_cost_is_recomputed_instead_of_persisted() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let lower = message(Some("request"), "session", 1_713_398_400_000, 10);
    let mut higher = lower.clone();
    higher.cost = 2.5;

    let capture = reconcile_at(&path, &[higher, lower], OBSERVED_AT).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.messages[0].cost, 0.0);
    assert_eq!(capture.messages[0].cost_source, CostSource::Unknown);
    assert_eq!(capture.conflicts, 0);
}
