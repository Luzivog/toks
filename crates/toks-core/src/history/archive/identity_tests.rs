use chrono::{TimeZone, Utc};
use tempfile::tempdir;
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::sessions::{
    AccountingAlias, AccountingAliasScheme, CostSource, DurableIdentity, DurableIdentityScheme,
    IdentityStrength, UnifiedMessage,
};
use toks_ingest::TokenBreakdown;

use super::{reconcile_at, schema};
use crate::history::materialize;

const CAPTURED_AT: i64 = 1_776_508_800_000;

fn observation(value: &str, input: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        model_id: "gpt-test".into(),
        provider_id: "openai".into(),
        session_id: "session-a".into(),
        workspace_key: None,
        workspace_label: None,
        timestamp: 1_713_398_400_000,
        date: String::new(),
        tokens: TokenBreakdown {
            input,
            output: 5,
            cache_read: 20,
            cache_write: 2,
            reasoning: 3,
        },
        cost: 1.0,
        cost_source: CostSource::Estimated,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some(format!("mutable:{input}")),
        durable_identity: Some(DurableIdentity {
            scheme: DurableIdentityScheme::CodexSessionRecordSequence,
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

fn fork_alias(value: &str) -> AccountingAlias {
    AccountingAlias {
        scheme: AccountingAliasScheme::CodexForkReplayDedup,
        version: 1,
        value: value.into(),
    }
}

#[test]
fn durable_identity_quarantines_reparsed_accounting_correction() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let original = observation("record-1", 10);
    let corrected = observation("record-1", 50);

    reconcile_at(&path, &[original], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[corrected], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.messages[0].tokens.input, 10);
    assert_eq!(capture.conflicts, 1);
    assert_eq!(capture.strong_events, 0);
    assert_eq!(capture.weak_events, 1);
}

#[test]
fn same_session_duplicate_with_the_same_durable_identity_counts_once() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let event = observation("record-1", 10);

    let capture = reconcile_at(&path, &[event.clone(), event], CAPTURED_AT).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.conflicts, 0);
}

#[test]
fn durable_identity_converges_across_clients_and_sessions() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut first = observation("provider-response-1", 10);
    first.client = "claude".into();
    first.durable_identity.as_mut().unwrap().scheme = DurableIdentityScheme::ClaudeProviderResponse;
    first.durable_identity.as_mut().unwrap().strength = IdentityStrength::Strong;
    let mut copy = first.clone();
    copy.client = "cc-mirror/import".into();
    copy.session_id = "copied-session".into();

    let capture = reconcile_at(&path, &[first, copy], CAPTURED_AT).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.strong_events, 1);
    let connection = schema::open(&path).unwrap();
    let sources: i64 = connection
        .query_row("SELECT COUNT(*) FROM event_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sources, 2);
}

#[test]
fn parent_replay_across_fork_children_counts_once_but_child_usage_is_distinct() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut replay_from_child_a = observation("child-a-primary", 10);
    replay_from_child_a.session_id = "child-a".into();
    let mut replay_from_child_b = replay_from_child_a.clone();
    replay_from_child_b.session_id = "child-b".into();
    replay_from_child_b.timestamp += 3_600_000;
    replay_from_child_b.durable_identity.as_mut().unwrap().value = "child-b-primary".into();
    replay_from_child_a.accounting_aliases = vec![fork_alias("shared-parent-replay")];
    replay_from_child_b.accounting_aliases = vec![fork_alias("shared-parent-replay")];

    reconcile_at(&path, &[replay_from_child_a], CAPTURED_AT).unwrap();
    let replayed = reconcile_at(&path, &[replay_from_child_b], CAPTURED_AT + 1).unwrap();

    assert_eq!(replayed.messages.len(), 1);
    assert_eq!(replayed.messages[0].tokens.input, 10);
    assert_eq!(replayed.conflicts, 0);

    let mut child_owned = observation("child-b-event", 10);
    child_owned.session_id = "child-b".into();
    child_owned.accounting_aliases = vec![fork_alias("child-b-owned")];
    let capture = reconcile_at(&path, &[child_owned], CAPTURED_AT + 2).unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(
        capture
            .messages
            .iter()
            .map(|message| message.tokens.input)
            .sum::<i64>(),
        20
    );
    assert_eq!(capture.weak_events, 2);
    let connection = schema::open(&path).unwrap();
    let sources: i64 = connection
        .query_row("SELECT COUNT(*) FROM event_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sources, 3);
    let aliases: i64 = connection
        .query_row("SELECT COUNT(*) FROM accounting_aliases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(aliases, 2);
}

#[test]
fn incompatible_facts_sharing_an_alias_are_quarantined_not_merged() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut first = observation("first-primary", 10);
    first.accounting_aliases = vec![fork_alias("colliding-alias")];
    let mut second = observation("second-primary", 50);
    second.accounting_aliases = vec![fork_alias("colliding-alias")];

    reconcile_at(&path, &[first], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[second], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(capture.conflicts, 2);
    let connection = schema::open(&path).unwrap();
    let conflicted: i64 = connection
        .query_row("SELECT conflicted FROM accounting_aliases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(conflicted, 1);
}

#[test]
fn primary_identity_disagreeing_with_an_alias_is_quarantined() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut first = observation("first-primary", 10);
    first.accounting_aliases = vec![fork_alias("first-alias")];
    let mut second = observation("second-primary", 10);
    second.accounting_aliases = vec![fork_alias("second-alias")];
    let mut contradiction = observation("first-primary", 10);
    contradiction.accounting_aliases = vec![fork_alias("second-alias")];

    reconcile_at(&path, &[first, second], CAPTURED_AT).unwrap();
    let capture = reconcile_at(&path, &[contradiction], CAPTURED_AT + 1).unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(capture.conflicts, 2);
}

#[test]
fn legacy_claude_key_and_typed_identity_share_one_event() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut legacy = observation("unused", 10);
    legacy.client = "cc-mirror/import".into();
    legacy.durable_identity = None;
    legacy.dedup_key = Some("message-1:request-1".into());
    let mut typed = legacy.clone();
    typed.session_id = "copied-session".into();
    typed.durable_identity = Some(DurableIdentity {
        scheme: DurableIdentityScheme::ClaudeProviderResponse,
        version: 1,
        value: "message-1:request-1".into(),
        strength: IdentityStrength::Strong,
    });

    let capture = reconcile_at(&path, &[legacy, typed], CAPTURED_AT).unwrap();

    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.strong_events, 1);
}

#[test]
fn mutable_codex_dedup_key_is_not_a_durable_identity() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut event = observation("unused", 10);
    event.durable_identity = None;

    let capture = reconcile_at(&path, &[event], CAPTURED_AT).unwrap();

    assert_eq!(capture.strong_events, 0);
    assert_eq!(capture.weak_events, 1);
}

#[test]
fn distinct_path_scoped_tool_results_in_one_session_do_not_collapse() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut first = observation("unused", 10);
    first.client = "cc-mirror/import".into();
    first.durable_identity = None;
    first.dedup_key = Some("claude:tool_result:session-a:tool-a".into());
    let mut second = first.clone();
    second.dedup_key = Some("claude:tool_result:session-a:tool-b".into());

    let capture = reconcile_at(&path, &[first, second], CAPTURED_AT).unwrap();

    assert_eq!(capture.messages.len(), 2);
    assert_eq!(capture.weak_events, 2);
}

#[test]
fn empty_rescan_preserves_every_materialized_total() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let event = observation("record-1", 10);
    let timezone = BucketTimezone::from_pinned_name(Some("UTC"));
    let now = Utc
        .with_ymd_and_hms(2026, 4, 18, 12, 0, 0)
        .single()
        .unwrap();

    let first_capture = reconcile_at(&path, &[event], CAPTURED_AT).unwrap();
    let first = materialize::snapshot(first_capture, now, &timezone, None);
    let second_capture = reconcile_at(&path, &[], CAPTURED_AT + 1).unwrap();
    let second = materialize::snapshot(second_capture, now, &timezone, None);

    assert_eq!(first.usage.daily[0].tokens, second.usage.daily[0].tokens);
    assert_eq!(
        first.usage.monthly[0].tokens,
        second.usage.monthly[0].tokens
    );
    assert_eq!(
        first.sources[0].total_tokens,
        second.sources[0].total_tokens
    );
    assert_eq!(
        first.sources[0].models[0].tokens,
        second.sources[0].models[0].tokens
    );
}

#[test]
fn typed_identity_is_stored_only_as_a_hash() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    reconcile_at(
        &path,
        &[observation("private-provider-id", 10)],
        CAPTURED_AT,
    )
    .unwrap();
    let connection = schema::open(&path).unwrap();
    let stored: String = connection
        .query_row("SELECT identity_hash FROM identities", [], |row| row.get(0))
        .unwrap();
    assert!(!stored.contains("private-provider-id"));
    let projection: i64 = connection
        .query_row(
            "SELECT accounting_projection_version FROM event_revisions",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(projection, 2);
}
