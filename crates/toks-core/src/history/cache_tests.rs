use std::fs;

use serde_json::json;

use super::cache::{load_at, store_at_for_test, store_at_with_limit_for_test};
use super::{HistorySnapshot, SourceHistory, UsageBucket, UsageSeries};

fn snapshot(tokens: i64) -> HistorySnapshot {
    let bucket = UsageBucket {
        key: "2026-08-18".into(),
        input: tokens,
        tokens,
        cost: 1.25,
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![SourceHistory {
            client: "codex".into(),
            usage: UsageSeries {
                daily: vec![bucket.clone()],
                ..Default::default()
            },
            total_tokens: tokens,
            total_cost: 1.25,
            ..Default::default()
        }],
        usage: UsageSeries {
            daily: vec![bucket],
            ..Default::default()
        },
        generated_at_ms: 1_776_729_600_000,
        ..Default::default()
    }
}

#[test]
fn aggregate_cache_round_trips_atomically_with_private_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let cache_dir = directory.path().join("history");
    let path = cache_dir.join("snapshot.json");
    store_at_for_test(&path, &snapshot(42)).unwrap();

    let hydrated = load_at(&path).unwrap();
    assert_eq!(hydrated.usage.daily[0].tokens, 42);
    assert_eq!(fs::read_dir(&cache_dir).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&cache_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn corrupt_and_incompatible_snapshots_are_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");

    fs::write(&path, b"not json").unwrap();
    assert!(load_at(&path).is_none());

    fs::write(
        &path,
        json!({"version": 99, "snapshot": snapshot(3)}).to_string(),
    )
    .unwrap();
    assert!(load_at(&path).is_none());
}

#[test]
fn invalid_replacement_preserves_the_last_good_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    store_at_for_test(&path, &snapshot(42)).unwrap();

    let mut invalid = snapshot(-1);
    invalid.usage.daily[0].input = -1;
    assert!(store_at_for_test(&path, &invalid).is_err());
    assert_eq!(load_at(&path).unwrap().usage.daily[0].tokens, 42);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn oversized_replacement_preserves_the_last_good_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    store_at_for_test(&path, &snapshot(42)).unwrap();
    let original = fs::read(&path).unwrap();

    assert!(store_at_with_limit_for_test(&path, &snapshot(84), 64).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn persisted_schema_contains_only_aggregate_history_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    store_at_for_test(&path, &snapshot(42)).unwrap();
    let raw = fs::read_to_string(path).unwrap();

    assert!(!raw.contains("email"));
    assert!(!raw.contains("account"));
    assert!(!raw.contains("config_dir"));
    assert!(!raw.contains("raw_path"));
}

#[test]
fn malformed_aggregate_metrics_are_rejected_on_hydration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    let mut value = serde_json::to_value(snapshot(42)).unwrap();
    value["usage"]["daily"][0]["tokens"] = json!(-42);
    fs::write(&path, json!({"version": 1, "snapshot": value}).to_string()).unwrap();

    assert!(load_at(&path).is_none());
}
