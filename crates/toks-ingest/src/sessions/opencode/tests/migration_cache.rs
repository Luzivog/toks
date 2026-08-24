use super::*;
use crate::paths::test_env::EnvGuard;

// -------------------------------------------------------------------------
// Migration cache tests
// -------------------------------------------------------------------------

/// Round-trip: save then load returns identical data.
#[test]
fn test_migration_cache_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    // Point the cache at a temp dir by overriding via a temporary env var is
    // impractical here; instead we test the structs and serde directly.
    let cache = OpenCodeMigrationCache {
        migration_complete: true,
        json_file_count: 42,
        json_dir_mtime_secs: 1_700_000_000,
        checked_at_secs: 1_700_100_000,
    };

    let json = serde_json::to_string(&cache).unwrap();
    let loaded: OpenCodeMigrationCache = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, cache);

    // Ensure the JSON contains all expected keys
    assert!(json.contains("migration_complete"));
    assert!(json.contains("json_file_count"));
    assert!(json.contains("json_dir_mtime_secs"));
    assert!(json.contains("checked_at_secs"));

    drop(dir);
}

/// Cache is valid when file count and mtime are unchanged.
#[test]
fn test_migration_cache_valid_when_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let json_dir = dir.path().join("message");
    std::fs::create_dir_all(&json_dir).unwrap();

    // Write a dummy file so the directory exists and has a stable mtime
    std::fs::write(json_dir.join("msg.json"), b"{}").unwrap();

    let current_mtime = get_json_dir_mtime(&json_dir).expect("should stat dir");
    let current_file_count = 1u64;

    let cache = OpenCodeMigrationCache {
        migration_complete: true,
        json_file_count: current_file_count,
        json_dir_mtime_secs: current_mtime, // same mtime
        checked_at_secs: now_secs(),
    };

    // Simulate the validity check from lib.rs
    let is_valid = cache.migration_complete
        && current_file_count == cache.json_file_count
        && get_json_dir_mtime(&json_dir).is_some_and(|m| m <= cache.json_dir_mtime_secs);

    assert!(is_valid, "Cache should be valid when count and mtime match");
}

/// Cache is invalid when file count has changed.
#[test]
fn test_migration_cache_invalid_when_file_count_changes() {
    let dir = tempfile::tempdir().unwrap();
    let json_dir = dir.path().join("message");
    std::fs::create_dir_all(&json_dir).unwrap();
    std::fs::write(json_dir.join("msg1.json"), b"{}").unwrap();

    let current_mtime = get_json_dir_mtime(&json_dir).unwrap();

    let cache = OpenCodeMigrationCache {
        migration_complete: true,
        json_file_count: 1,
        json_dir_mtime_secs: current_mtime,
        checked_at_secs: now_secs(),
    };

    // Simulate: a new file was added → current_file_count = 2
    let current_file_count = 2u64; // changed
    let is_valid = cache.migration_complete
        && current_file_count == cache.json_file_count
        && get_json_dir_mtime(&json_dir).is_some_and(|m| m <= cache.json_dir_mtime_secs);

    assert!(!is_valid, "Cache should be invalid when file count changes");
}

/// Cache is invalid when directory mtime is newer than cached value.
#[test]
fn test_migration_cache_invalid_when_mtime_newer() {
    let dir = tempfile::tempdir().unwrap();
    let json_dir = dir.path().join("message");
    std::fs::create_dir_all(&json_dir).unwrap();
    std::fs::write(json_dir.join("msg.json"), b"{}").unwrap();

    let current_mtime = get_json_dir_mtime(&json_dir).unwrap();

    // Simulate: cache recorded an older mtime → directory is now newer
    let stale_mtime = current_mtime.saturating_sub(1);
    let cache = OpenCodeMigrationCache {
        migration_complete: true,
        json_file_count: 1,
        json_dir_mtime_secs: stale_mtime, // older than current
        checked_at_secs: now_secs(),
    };

    let is_valid = cache.migration_complete
        && 1u64 == cache.json_file_count
        && get_json_dir_mtime(&json_dir).is_some_and(|m| m <= cache.json_dir_mtime_secs);

    assert!(
        !is_valid,
        "Cache should be invalid when directory mtime is newer than cached value"
    );
}

/// Cache is not loaded when the file is missing (load returns None).
#[test]
fn test_migration_cache_missing_returns_none() {
    // load_opencode_migration_cache reads from ~/.cache/tokscope/opencode-migration.json
    // We can't easily override the path in a unit test, but we can verify that
    // serde_json::from_str returns None for invalid input (simulating missing file).
    let result: Option<OpenCodeMigrationCache> = serde_json::from_str("").ok();
    assert!(
        result.is_none(),
        "Empty/missing content should produce None"
    );
}

/// migration_complete=false disables the cache even if count/mtime match.
#[test]
fn test_migration_cache_not_skipped_when_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let json_dir = dir.path().join("message");
    std::fs::create_dir_all(&json_dir).unwrap();
    std::fs::write(json_dir.join("msg.json"), b"{}").unwrap();

    let current_mtime = get_json_dir_mtime(&json_dir).unwrap();

    let cache = OpenCodeMigrationCache {
        migration_complete: false, // migration not complete
        json_file_count: 1,
        json_dir_mtime_secs: current_mtime,
        checked_at_secs: now_secs(),
    };

    let is_valid = cache.migration_complete
        && 1u64 == cache.json_file_count
        && get_json_dir_mtime(&json_dir).is_some_and(|m| m <= cache.json_dir_mtime_secs);

    assert!(
        !is_valid,
        "Cache should not allow skipping when migration_complete=false"
    );
}

#[test]
#[serial_test::serial]
fn migration_record_falls_back_to_legacy_path() {
    let temp_home = tempfile::tempdir().unwrap();
    let temp_xdg_cache = tempfile::tempdir().unwrap();
    let mut guard = EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR", "XDG_CACHE_HOME", "HOME"]);
    guard.set("HOME", temp_home.path());
    guard.set("XDG_CACHE_HOME", temp_xdg_cache.path());
    guard.remove("TOKSCOPE_CONFIG_DIR");

    let legacy_path = crate::paths::legacy_dirs_cache_dir()
        .unwrap()
        .join(MIGRATION_CACHE_FILENAME);
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::write(
        &legacy_path,
        r#"{"migration_complete":true,"json_file_count":2,"json_dir_mtime_secs":3,"checked_at_secs":4}"#,
    )
    .unwrap();

    let loaded = load_opencode_migration_cache().unwrap();
    assert!(loaded.migration_complete);
    assert_eq!(loaded.json_file_count, 2);
}
