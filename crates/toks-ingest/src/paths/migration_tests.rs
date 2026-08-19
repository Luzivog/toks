use std::fs;

use serial_test::serial;

use super::migration::{is_legacy_executable, resolved_root};
use super::test_env::EnvGuard;

fn guard() -> EnvGuard {
    EnvGuard::capture(&["TOKS_TEST_LEGACY_PROCESS_RUNNING"])
}

#[test]
fn deleted_legacy_executable_is_still_recognized() {
    assert!(is_legacy_executable(
        "/home/user/.local/bin/tokscope (deleted)"
    ));
    assert!(is_legacy_executable("/usr/bin/tokscope"));
    assert!(!is_legacy_executable("/usr/bin/toks"));
}

#[test]
#[serial]
fn legacy_only_root_migrates_once_without_losing_nested_state() {
    let mut env = guard();
    env.set("TOKS_TEST_LEGACY_PROCESS_RUNNING", "0");
    let root = tempfile::tempdir().unwrap();
    let legacy = root.path().join("tokscope");
    let current = root.path().join("toks");
    fs::create_dir_all(legacy.join("history")).unwrap();
    fs::create_dir_all(legacy.join("profiles/account-a")).unwrap();
    fs::write(legacy.join("history/usage.sqlite3"), b"archive").unwrap();
    fs::write(legacy.join("profiles/account-a/profile.json"), b"profile").unwrap();

    assert_eq!(resolved_root(legacy.clone(), current.clone()), current);
    assert!(!legacy.exists());
    assert_eq!(
        fs::read(root.path().join("toks/history/usage.sqlite3")).unwrap(),
        b"archive"
    );
    assert_eq!(
        fs::read(root.path().join("toks/profiles/account-a/profile.json")).unwrap(),
        b"profile"
    );
}

#[test]
#[serial]
fn coexistence_migration_is_idempotent_and_preserves_conflicts() {
    let mut env = guard();
    env.set("TOKS_TEST_LEGACY_PROCESS_RUNNING", "0");
    let root = tempfile::tempdir().unwrap();
    let legacy = root.path().join("tokscope");
    let current = root.path().join("toks");
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&current).unwrap();
    fs::write(legacy.join("settings.json"), b"legacy-conflict").unwrap();
    fs::write(current.join("settings.json"), b"current-conflict").unwrap();
    fs::write(legacy.join("cache.json"), b"legacy-only").unwrap();

    assert_eq!(resolved_root(legacy.clone(), current.clone()), current);
    assert_eq!(
        fs::read(current.join("settings.json")).unwrap(),
        b"current-conflict"
    );
    assert_eq!(
        fs::read(current.join("cache.json")).unwrap(),
        b"legacy-only"
    );
    assert_eq!(
        fs::read(legacy.join("settings.json")).unwrap(),
        b"legacy-conflict"
    );
    assert_eq!(resolved_root(legacy.clone(), current.clone()), current);
    assert_eq!(
        fs::read(legacy.join("settings.json")).unwrap(),
        b"legacy-conflict"
    );
}

#[test]
#[serial]
fn running_legacy_process_defers_migration_and_uses_one_root() {
    let mut env = guard();
    env.set("TOKS_TEST_LEGACY_PROCESS_RUNNING", "1");
    let root = tempfile::tempdir().unwrap();
    let legacy = root.path().join("tokscope");
    let current = root.path().join("toks");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("settings.json"), b"live").unwrap();

    assert_eq!(resolved_root(legacy.clone(), current.clone()), legacy);
    assert!(!current.exists());
}
