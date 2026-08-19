use tempfile::tempdir;
use toks_ingest::sessions::{
    CostSource, DurableIdentity, DurableIdentityScheme, IdentityStrength, UnifiedMessage,
};
use toks_ingest::TokenBreakdown;

use super::{checkpoint, load_at, schema, SourceDelta};

const OBSERVED_AT: i64 = 1_776_508_800_000;

#[test]
fn unchanged_source_revision_performs_zero_writes() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = [message("one", 10)];
    let mut connection = schema::open(&path).unwrap();
    checkpoint::apply(
        &mut connection,
        delta("revision-one", &observations, true),
        OBSERVED_AT,
    )
    .unwrap();
    let before: i64 = connection
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();

    let outcome = checkpoint::apply_with_report(
        &mut connection,
        delta("revision-one", &observations, true),
        OBSERVED_AT + 1,
    )
    .unwrap();
    let after: i64 = connection
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();

    assert!(!outcome.changed);
    assert!(outcome.changes.is_empty());
    assert_eq!(after, before);
}

#[test]
fn one_source_delta_adds_only_the_new_canonical_event() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = [message("one", 10)];
    let second = [message("one", 10), message("two", 20)];
    let mut connection = schema::open(&path).unwrap();
    checkpoint::apply(
        &mut connection,
        delta("revision-one", &first, true),
        OBSERVED_AT,
    )
    .unwrap();

    let outcome = checkpoint::apply_with_report(
        &mut connection,
        delta("revision-two", &second, true),
        OBSERVED_AT + 1,
    )
    .unwrap();

    assert_eq!(outcome.changes.len(), 1);
    assert!(outcome.changes[0].before.is_none());
    assert_eq!(outcome.changes[0].after.as_ref().unwrap().input, 20);
    assert_eq!(count(&connection, "events"), 2);
}

#[test]
fn checkpoint_and_events_commit_or_roll_back_together() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = [message("one", 10)];
    let mut connection = schema::open(&path).unwrap();

    assert!(checkpoint::apply_then_interrupt(
        &mut connection,
        delta("revision-one", &observations, false),
        OBSERVED_AT,
    )
    .is_err());
    assert_eq!(count(&connection, "events"), 0);
    assert_eq!(count(&connection, "source_checkpoints"), 0);

    checkpoint::apply(
        &mut connection,
        delta("revision-one", &observations, false),
        OBSERVED_AT,
    )
    .unwrap();
    drop(connection);
    let capture = load_at(&path).unwrap().unwrap();
    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.pending_sources, 1);
}

#[test]
fn empty_changed_source_never_deletes_accepted_usage() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = [message("one", 10)];
    let mut connection = schema::open(&path).unwrap();
    checkpoint::apply(
        &mut connection,
        delta("revision-one", &observations, true),
        OBSERVED_AT,
    )
    .unwrap();
    checkpoint::apply(
        &mut connection,
        delta("revision-empty", &[], true),
        OBSERVED_AT + 1,
    )
    .unwrap();
    drop(connection);

    let capture = load_at(&path).unwrap().unwrap();
    assert_eq!(capture.messages.len(), 1);
    assert_eq!(capture.messages[0].tokens.input, 10);
}

#[test]
fn canonical_completion_replaces_projection_instead_of_double_counting() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let mut partial = message("response", 10);
    partial.client = "claude".into();
    partial.provider_id = "anthropic".into();
    partial.durable_identity = Some(DurableIdentity {
        scheme: DurableIdentityScheme::ClaudeProviderResponse,
        version: 1,
        value: "response".into(),
        strength: IdentityStrength::Strong,
    });
    let mut full = partial.clone();
    full.tokens.input = 20;
    let mut connection = schema::open(&path).unwrap();
    checkpoint::apply(
        &mut connection,
        delta("partial", std::slice::from_ref(&partial), true),
        OBSERVED_AT,
    )
    .unwrap();

    let report = checkpoint::apply_with_report(
        &mut connection,
        delta("full", std::slice::from_ref(&full), true),
        OBSERVED_AT + 1,
    )
    .unwrap();
    let projected: i64 = connection
        .query_row(
            "SELECT SUM(input_tokens) FROM usage_rollups WHERE period=0",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(report.changes.len(), 1);
    assert_eq!(report.changes[0].before.as_ref().unwrap().input, 10);
    assert_eq!(report.changes[0].after.as_ref().unwrap().input, 20);
    assert_eq!(projected, 20);
}

fn delta<'a>(
    revision: &'a str,
    observations: &'a [UnifiedMessage],
    complete: bool,
) -> SourceDelta<'a> {
    SourceDelta {
        source_key: "transient/raw/source/name",
        revision,
        observations,
        backfill_complete: complete,
    }
}

fn message(key: &str, input: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        provider_id: "openai".into(),
        model_id: "gpt-test".into(),
        session_id: "session".into(),
        timestamp: 1_713_398_400_000 + input,
        date: String::new(),
        tokens: TokenBreakdown {
            input,
            output: 5,
            cache_read: 20,
            cache_write: 2,
            reasoning: 3,
        },
        cost: 0.0,
        cost_source: CostSource::Unknown,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some(key.into()),
        durable_identity: None,
        accounting_aliases: Vec::new(),
        workspace_key: None,
        workspace_label: None,
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}

fn count(connection: &rusqlite::Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
