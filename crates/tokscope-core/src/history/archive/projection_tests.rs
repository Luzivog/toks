use tempfile::tempdir;
use tokscope_ingest::sessions::{CostSource, UnifiedMessage};
use tokscope_ingest::TokenBreakdown;

use super::{load, projection_load, projection_migration, reconcile_at, schema};

const OBSERVED_AT: i64 = 1_776_508_800_000;

#[test]
fn version_two_events_migrate_to_exact_rollup_totals() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = message(0, 10, 0.0, CostSource::Estimated);
    let second = message(1, 20, 1.75, CostSource::ProviderReported);
    reconcile_at(&path, &[first, second], OBSERVED_AT).unwrap();
    downgrade_projection(&path);

    let mut connection = schema::open(&path).unwrap();
    assert!(!projection_migration::is_complete(&connection).unwrap());
    assert_eq!(projection_migration::pending_count(&connection).unwrap(), 2);
    projection_migration::advance(&mut connection).unwrap();

    assert!(projection_migration::is_complete(&connection).unwrap());
    let projection = projection_load::load(&connection).unwrap();
    assert_eq!(projection.strong_events, 0);
    assert_eq!(projection.weak_events, 2);
    assert_eq!(projection.conflicts, 0);
    let totals: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT SUM(input_tokens), SUM(output_tokens), SUM(cost_nanos), SUM(event_count)
             FROM usage_rollups WHERE period=0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(totals, (30, 10, 1_750_000_000, 2));
}

#[test]
fn incomplete_v2_projection_is_explicit_until_atomic_migration() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = (0..300)
        .map(|index| message(index, 10, 0.0, CostSource::Unknown))
        .collect::<Vec<_>>();
    reconcile_at(&path, &observations, OBSERVED_AT).unwrap();
    downgrade_projection(&path);

    let mut connection = schema::open(&path).unwrap();
    let before = projection_load::load(&connection).unwrap();
    let capture = load::capture(&connection).unwrap();
    assert_eq!(capture.messages.len(), 300);
    assert_eq!(before.projection_pending, 300);
    assert!(before.rollups.is_empty());

    assert_eq!(projection_migration::advance(&mut connection).unwrap(), 300);
    let after = projection_load::load(&connection).unwrap();
    assert!(after.projection_complete);
    assert_eq!(after.projection_pending, 0);
    assert!(!after.rollups.is_empty());
    assert_eq!(projection_rows(&connection), 2);
}

#[test]
fn projection_migration_is_replay_safe() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    reconcile_at(
        &path,
        &[message(0, 10, 2.0, CostSource::ProviderReported)],
        OBSERVED_AT,
    )
    .unwrap();
    downgrade_projection(&path);
    let mut connection = schema::open(&path).unwrap();

    projection_migration::advance(&mut connection).unwrap();
    let before: (i64, i64) = connection
        .query_row(
            "SELECT input_tokens, cost_nanos FROM usage_rollups WHERE period=0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    projection_migration::advance(&mut connection).unwrap();
    let after: (i64, i64) = connection
        .query_row(
            "SELECT input_tokens, cost_nanos FROM usage_rollups WHERE period=0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(after, before);
}

#[test]
fn compact_pricing_basis_preserves_request_context_classes() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = message(0, 200_000, 0.0, CostSource::Estimated);
    let second = message(1, 200_000, 0.0, CostSource::Estimated);
    let long = message(2, 300_000, 0.0, CostSource::Estimated);
    reconcile_at(&path, &[first, second, long], OBSERVED_AT).unwrap();

    let connection = schema::open(&path).unwrap();
    let projection = projection_load::load(&connection).unwrap();
    let standard = projection
        .rollups
        .iter()
        .find(|rollup| rollup.bucket_start_ms == 0 && !rollup.long_context)
        .unwrap();
    let long = projection
        .rollups
        .iter()
        .find(|rollup| rollup.bucket_start_ms == 0 && rollup.long_context)
        .unwrap();

    assert_eq!(standard.pricing_basis.input, [256_000, 144_000, 0, 0, 0]);
    assert_eq!(
        long.pricing_basis.input,
        [128_000, 72_000, 56_000, 16_000, 28_000]
    );
}

#[test]
fn migration_normalizes_legacy_codex_output_without_losing_reasoning() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    reconcile_at(
        &path,
        &[message(0, 10, 0.0, CostSource::Estimated)],
        OBSERVED_AT,
    )
    .unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "UPDATE events SET output_tokens=8, reasoning_tokens=3;
             UPDATE event_revisions SET
               output_tokens=8, reasoning_tokens=3, accounting_projection_version=1;",
        )
        .unwrap();
    drop(connection);
    downgrade_projection(&path);

    let mut connection = schema::open(&path).unwrap();
    projection_migration::advance(&mut connection).unwrap();
    let projection = projection_load::load(&connection).unwrap();
    let all = projection
        .rollups
        .iter()
        .find(|rollup| rollup.bucket_start_ms == 0)
        .unwrap();

    assert_eq!(all.output, 5);
    assert_eq!(all.reasoning, 3);
    assert_eq!(all.pricing_basis.output.iter().sum::<i64>(), 8);
}

#[test]
fn set_based_projection_migrates_a_large_archive_in_one_transaction() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = (0..5_000)
        .map(|index| message(index, 1, 0.0, CostSource::Unknown))
        .collect::<Vec<_>>();
    reconcile_at(&path, &observations, OBSERVED_AT).unwrap();
    downgrade_projection(&path);
    let mut connection = schema::open(&path).unwrap();

    let migrated = projection_migration::advance(&mut connection).unwrap();

    assert_eq!(migrated, 5_000);
    assert_eq!(projection_migration::pending_count(&connection).unwrap(), 0);
    assert!(projection_migration::is_complete(&connection).unwrap());
    assert_eq!(projection_migration::advance(&mut connection).unwrap(), 0);
}

fn downgrade_projection(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            "DROP INDEX projection_events_by_fact;
             DROP TABLE usage_rollups;
             DROP TABLE projection_events;
             DROP TABLE projection_state;
             DROP TABLE source_checkpoints;
             PRAGMA user_version=2;",
        )
        .unwrap();
}

fn projection_rows(connection: &rusqlite::Connection) -> i64 {
    connection
        .query_row("SELECT COUNT(*) FROM usage_rollups", [], |row| row.get(0))
        .unwrap()
}

fn message(index: usize, input: i64, cost: f64, source: CostSource) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        provider_id: "openai".into(),
        model_id: "gpt-test".into(),
        session_id: "session".into(),
        timestamp: 1_713_398_400_000 + index as i64,
        date: String::new(),
        tokens: TokenBreakdown {
            input,
            output: 5,
            cache_read: 20,
            cache_write: 2,
            reasoning: 3,
        },
        cost,
        cost_source: source,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some(format!("event-{index}")),
        durable_identity: None,
        accounting_aliases: Vec::new(),
        workspace_key: None,
        workspace_label: None,
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}
