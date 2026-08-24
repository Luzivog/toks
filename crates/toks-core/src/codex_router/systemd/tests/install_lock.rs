use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;

use super::super::LifecycleGuard;

const CHILD_TEST: &str = "codex_router::systemd::tests::install_lock::installer_process";

#[test]
fn two_installer_processes_cannot_overlap_the_systemd_lifecycle() {
    let directory = tempdir().unwrap();
    let lock = directory.path().join("install.lock");
    let first_attempted = directory.path().join("first-attempted");
    let first_entered = directory.path().join("first-entered");
    let first_release = directory.path().join("first-release");
    let mut first = installer(&lock, &first_attempted, &first_entered, &first_release);
    wait_for(&first_entered);

    let second_attempted = directory.path().join("second-attempted");
    let second_entered = directory.path().join("second-entered");
    let second_release = directory.path().join("second-release");
    let mut second = installer(&lock, &second_attempted, &second_entered, &second_release);
    wait_for(&second_attempted);
    thread::sleep(Duration::from_millis(100));
    assert!(!second_entered.exists());
    assert!(second.try_wait().unwrap().is_none());

    std::fs::write(first_release, []).unwrap();
    assert!(first.wait().unwrap().success());
    wait_for(&second_entered);
    std::fs::write(second_release, []).unwrap();
    assert!(second.wait().unwrap().success());
}

#[test]
fn installer_process() {
    let Ok(lock) = std::env::var("TOKS_TEST_LIFECYCLE_LOCK") else {
        return;
    };
    let attempted = PathBuf::from(std::env::var_os("TOKS_TEST_ATTEMPTED").unwrap());
    let entered = PathBuf::from(std::env::var_os("TOKS_TEST_ENTERED").unwrap());
    let release = PathBuf::from(std::env::var_os("TOKS_TEST_RELEASE").unwrap());
    std::fs::write(attempted, []).unwrap();
    let _guard = LifecycleGuard::acquire_at(Path::new(&lock)).unwrap();
    std::fs::write(entered, []).unwrap();
    wait_for(&release);
}

fn installer(lock: &Path, attempted: &Path, entered: &Path, release: &Path) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .env("TOKS_TEST_LIFECYCLE_LOCK", lock)
        .env("TOKS_TEST_ATTEMPTED", attempted)
        .env("TOKS_TEST_ENTERED", entered)
        .env("TOKS_TEST_RELEASE", release)
        .spawn()
        .unwrap()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}
