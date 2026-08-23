use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use rusqlite::Connection;

use super::desktop_snapshot_at;
use crate::remote_control::{RemoteConnectionStatus, RemoteControlOwner};

#[test]
fn matching_desktop_process_and_enrollment_claim_ownership() {
    let root = tempfile::tempdir().unwrap();
    let codex_home = root.path().join("codex-home");
    let proc_root = root.path().join("proc");
    create_enrollment(&codex_home, "current", true);
    create_desktop_process(&proc_root, &codex_home);

    let snapshot = desktop_snapshot_at(&codex_home, &proc_root, None).unwrap();

    assert_eq!(
        snapshot.connection.status,
        RemoteConnectionStatus::Managed(RemoteControlOwner::ChatGptDesktop)
    );
    assert_eq!(
        snapshot.connection.server_name.as_deref(),
        Some("workstation")
    );
}

#[test]
fn enrollment_for_another_account_does_not_claim_ownership() {
    let root = tempfile::tempdir().unwrap();
    let codex_home = root.path().join("codex-home");
    let proc_root = root.path().join("proc");
    create_enrollment(&codex_home, "other", true);
    create_desktop_process(&proc_root, &codex_home);

    assert!(desktop_snapshot_at(&codex_home, &proc_root, None).is_none());
}

#[test]
fn disabled_desktop_enrollment_does_not_claim_ownership() {
    let root = tempfile::tempdir().unwrap();
    let codex_home = root.path().join("codex-home");
    let proc_root = root.path().join("proc");
    create_enrollment(&codex_home, "current", false);
    create_desktop_process(&proc_root, &codex_home);

    assert!(desktop_snapshot_at(&codex_home, &proc_root, None).is_none());
}

#[test]
fn enrollment_without_a_live_desktop_app_server_does_not_claim_ownership() {
    let root = tempfile::tempdir().unwrap();
    let codex_home = root.path().join("codex-home");
    let proc_root = root.path().join("proc");
    create_enrollment(&codex_home, "current", true);
    fs::create_dir_all(&proc_root).unwrap();

    assert!(desktop_snapshot_at(&codex_home, &proc_root, None).is_none());
}

fn create_enrollment(codex_home: &Path, enrolled_account: &str, enabled: bool) {
    fs::create_dir_all(codex_home).unwrap();
    fs::write(
        codex_home.join("auth.json"),
        r#"{"tokens":{"account_id":"current"}}"#,
    )
    .unwrap();
    let connection = Connection::open(codex_home.join("state_5.sqlite")).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE remote_control_enrollments (
                account_id TEXT NOT NULL,
                app_server_client_name TEXT NOT NULL,
                server_name TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                remote_control_enabled INTEGER
            );",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO remote_control_enrollments VALUES (?1, 'Codex Desktop', \
             'workstation', 'environment', 1, ?2)",
            (enrolled_account, i64::from(enabled)),
        )
        .unwrap();
}

fn create_desktop_process(proc_root: &Path, codex_home: &Path) {
    let desktop = proc_root.join("100");
    let child = proc_root.join("101");
    fs::create_dir_all(desktop.join("task/100")).unwrap();
    fs::create_dir_all(&child).unwrap();
    symlink("/usr/lib/chatgpt/ChatGPT", desktop.join("exe")).unwrap();
    symlink("/usr/bin/codex", child.join("exe")).unwrap();
    fs::write(desktop.join("cmdline"), b"/usr/lib/chatgpt/ChatGPT\0").unwrap();
    fs::write(desktop.join("task/100/children"), b"101").unwrap();
    fs::write(child.join("cmdline"), b"codex\0app-server\0").unwrap();
    fs::write(
        child.join("environ"),
        format!("CODEX_HOME={}\0", codex_home.display()),
    )
    .unwrap();
}
