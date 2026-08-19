use std::fs;

use rusqlite::params;
use tempfile::tempdir;
use tokscope_ingest::sessions::{
    CostSource, DurableIdentity, DurableIdentityScheme, IdentityStrength, UnifiedMessage,
};
use tokscope_ingest::TokenBreakdown;

use super::{checkpoint, load_at, lookup, reconcile_at, schema, SourceDelta};

const OBSERVED_AT: i64 = 1_776_508_800_000;

fn observation(index: usize) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        model_id: "gpt-test".into(),
        provider_id: "openai".into(),
        session_id: format!("session-{index}"),
        workspace_key: None,
        workspace_label: None,
        timestamp: 1_713_398_400_000 + index as i64,
        date: String::new(),
        tokens: TokenBreakdown {
            input: 10 + index as i64,
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
        dedup_key: None,
        durable_identity: Some(DurableIdentity {
            scheme: DurableIdentityScheme::CodexSessionRecordSequence,
            version: 1,
            value: format!("record-{index}"),
            strength: IdentityStrength::SessionStable,
        }),
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}

#[test]
fn identical_rescan_performs_no_database_writes() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = (0..8).map(observation).collect::<Vec<_>>();
    let mut connection = schema::open(&path).unwrap();
    let delta = || SourceDelta {
        source_key: "source",
        revision: "revision",
        observations: &observations,
        backfill_complete: true,
    };
    checkpoint::apply(&mut connection, delta(), OBSERVED_AT).unwrap();
    let wal_before = wal_size(&path);
    let before: i64 = connection
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    checkpoint::apply(&mut connection, delta(), OBSERVED_AT).unwrap();
    let after: i64 = connection
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();

    assert_eq!(after, before);
    assert_eq!(wal_size(&path), wal_before);
}

#[test]
fn interrupted_source_keeps_prior_committed_sources_readable() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = (0..200).map(observation).collect::<Vec<_>>();
    let second = (200..300).map(observation).collect::<Vec<_>>();
    let mut connection = schema::open(&path).unwrap();
    checkpoint::apply(
        &mut connection,
        SourceDelta {
            source_key: "first",
            revision: "one",
            observations: &first,
            backfill_complete: false,
        },
        OBSERVED_AT,
    )
    .unwrap();
    assert!(checkpoint::apply_then_interrupt(
        &mut connection,
        SourceDelta {
            source_key: "second",
            revision: "one",
            observations: &second,
            backfill_complete: false,
        },
        OBSERVED_AT,
    )
    .is_err());
    drop(connection);
    let partial = load_at(&path).unwrap().unwrap();
    assert_eq!(partial.messages.len(), 200);
    let mut connection = schema::open(&path).unwrap();
    checkpoint::apply(
        &mut connection,
        SourceDelta {
            source_key: "second",
            revision: "one",
            observations: &second,
            backfill_complete: true,
        },
        OBSERVED_AT,
    )
    .unwrap();
    drop(connection);
    let capture = load_at(&path).unwrap().unwrap();
    let connection = schema::open(&path).unwrap();
    assert_eq!(capture.messages.len(), 300);
    assert_eq!(count(&connection, "events"), 300);
    assert_eq!(count(&connection, "event_revisions"), 300);
    assert_eq!(count(&connection, "source_checkpoints"), 2);
}

#[test]
fn version_one_migrates_to_current_schema_without_data_loss() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    reconcile_at(&path, &[observation(0)], OBSERVED_AT).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP INDEX revisions_by_accounting;
             DROP INDEX sources_by_source;
             DROP TABLE archive_pending;
             PRAGMA user_version=1;",
        )
        .unwrap();
    drop(connection);

    let connection = schema::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    assert_eq!(count(&connection, "events"), 1);
    assert!(object_exists(&connection, "table", "archive_pending"));
    assert!(object_exists(
        &connection,
        "index",
        "revisions_by_accounting"
    ));
    assert!(object_exists(&connection, "index", "sources_by_source"));
    assert!(object_exists(&connection, "table", "source_checkpoints"));
    assert!(object_exists(&connection, "table", "usage_rollups"));
}

#[test]
fn confidence_lookup_uses_bounded_index_probes() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let connection = schema::open(&path).unwrap();
    let sql = format!("EXPLAIN QUERY PLAN {}", lookup::DIFFERENT_CONFIDENCE_SQL);
    let mut statement = connection.prepare(&sql).unwrap();
    let details = statement
        .query_map(params!["accounting", 1, "source"], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");

    assert!(details.contains("revisions_by_accounting"), "{details}");
    assert!(!details.contains("SCAN r"), "{details}");
    assert!(details.contains("event_sources"), "{details}");
}

fn count(connection: &rusqlite::Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn object_exists(connection: &rusqlite::Connection, kind: &str, name: &str) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type=?1 AND name=?2)",
            params![kind, name],
            |row| row.get(0),
        )
        .unwrap()
}

fn wal_size(path: &std::path::Path) -> u64 {
    fs::metadata(format!("{}-wal", path.display()))
        .map(|metadata| metadata.len())
        .unwrap_or_default()
}
