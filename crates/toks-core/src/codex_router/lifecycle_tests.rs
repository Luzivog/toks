use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use super::systemd::LifecycleGuard;

const CHILD_TEST: &str = "codex_router::lifecycle_tests::lifecycle_process";

#[test]
fn enable_then_disable_is_one_cross_process_lifecycle_transaction() {
    let fixture = Fixture::new();
    let enable_release = fixture.root.path().join("enable-release");
    let mut enable = fixture.spawn("enable", Some(&enable_release));
    wait_for(&fixture.entered("enable"));

    let mut disable = fixture.spawn("disable", None);
    wait_for(&fixture.attempted("disable"));
    assert_blocked(&mut disable, &fixture.entered("disable"));

    std::fs::write(enable_release, []).unwrap();
    assert!(enable.wait().unwrap().success());
    assert!(disable.wait().unwrap().success());
    assert!(!fixture.enabled());
    assert!(!fixture.service().exists());
    assert!(!fixture.config().exists());
}

#[test]
fn updater_rechecks_disabled_intent_after_waiting_for_the_lifecycle_guard() {
    let fixture = Fixture::new();
    fixture.set_enabled(true);
    std::fs::write(fixture.service(), []).unwrap();
    let disable_release = fixture.root.path().join("disable-release");
    let mut disable = fixture.spawn("disable", Some(&disable_release));
    wait_for(&fixture.entered("disable"));

    let mut updater = fixture.spawn("updater", None);
    wait_for(&fixture.attempted("updater"));
    assert_blocked(&mut updater, &fixture.entered("updater"));

    std::fs::write(disable_release, []).unwrap();
    assert!(disable.wait().unwrap().success());
    assert!(updater.wait().unwrap().success());
    assert!(!fixture.enabled());
    assert!(!fixture.service().exists());
    assert!(!fixture.updater_install().exists());
}

#[test]
fn updater_observes_enabled_intent_only_after_enable_finishes() {
    let fixture = Fixture::new();
    let enable_release = fixture.root.path().join("enable-release");
    let mut enable = fixture.spawn("enable", Some(&enable_release));
    wait_for(&fixture.entered("enable"));

    let mut updater = fixture.spawn("updater", None);
    wait_for(&fixture.attempted("updater"));
    assert_blocked(&mut updater, &fixture.entered("updater"));

    std::fs::write(enable_release, []).unwrap();
    assert!(enable.wait().unwrap().success());
    assert!(updater.wait().unwrap().success());
    assert!(fixture.enabled());
    assert!(fixture.updater_install().exists());
}

#[test]
fn superseded_installer_cannot_replace_the_final_published_build() {
    let fixture = Fixture::new();
    fixture.set_enabled(true);
    fixture.publish("candidate-a");
    let guard = LifecycleGuard::acquire_at(&fixture.lock()).unwrap();
    let mut installer_a = fixture.spawn("installer-a", None);
    wait_for(&fixture.attempted("installer-a"));
    assert_blocked(&mut installer_a, &fixture.entered("installer-a"));

    fixture.publish("candidate-b");
    drop(guard);
    assert!(installer_a.wait().unwrap().success());
    assert!(!fixture.root.path().join("installer-a-install").exists());

    let mut installer_b = fixture.spawn("installer-b", None);
    assert!(installer_b.wait().unwrap().success());
    assert!(fixture.root.path().join("installer-b-install").exists());
}

#[test]
fn direct_manual_install_intentionally_uses_its_running_executable() {
    let fixture = Fixture::new();
    fixture.set_enabled(true);
    fixture.publish("candidate-b");

    let mut manual = fixture.spawn("manual-a", None);

    assert!(manual.wait().unwrap().success());
    assert!(fixture.root.path().join("manual-a-install").exists());
}

#[test]
fn lifecycle_process() {
    let Some(role) = std::env::var_os("TOKS_TEST_LIFECYCLE_ROLE") else {
        return;
    };
    let role = role.to_str().unwrap();
    let root = PathBuf::from(std::env::var_os("TOKS_TEST_LIFECYCLE_ROOT").unwrap());
    std::fs::write(root.join(format!("{role}-attempted")), []).unwrap();
    let guard = LifecycleGuard::acquire_at(&root.join("lifecycle.lock")).unwrap();
    std::fs::write(root.join(format!("{role}-entered")), []).unwrap();
    if let Some(release) = std::env::var_os("TOKS_TEST_LIFECYCLE_RELEASE") {
        wait_for(Path::new(&release));
    }
    match role {
        "enable" => {
            super::lifecycle::enable_locked(
                &guard,
                &root.join("router"),
                |_, _| {
                    std::fs::write(root.join("service"), [])?;
                    Ok(())
                },
                || {
                    std::fs::write(root.join("config"), [])?;
                    Ok(())
                },
            )
            .unwrap();
        }
        "disable" => {
            super::lifecycle::disable_locked(
                &guard,
                || {
                    remove(root.join("config"));
                    Ok(())
                },
                |_| {
                    remove(root.join("service"));
                    Ok(())
                },
            )
            .unwrap();
        }
        "updater" => {
            super::lifecycle::install_router_service_if_enabled(
                &guard,
                &root.join("router"),
                None,
                |_, _| {
                    std::fs::write(root.join("updater-install"), [])?;
                    Ok(())
                },
            )
            .unwrap();
        }
        "installer-a" | "installer-b" | "manual-a" => {
            let candidate = role.strip_prefix("installer-").unwrap_or("a");
            let candidate = root.join(format!("candidate-{candidate}"));
            let link = (role != "manual-a").then(|| root.join("installed-router"));
            super::lifecycle::install_router_service_if_enabled(
                &guard,
                &candidate,
                link.as_deref(),
                |_, _| {
                    std::fs::write(root.join(format!("{role}-install")), [])?;
                    Ok(())
                },
            )
            .unwrap();
        }
        _ => panic!("unknown lifecycle role"),
    }
}

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for name in ["router", "candidate-a", "candidate-b"] {
            std::fs::write(root.path().join(name), name).unwrap();
        }
        Self { root }
    }

    fn spawn(&self, role: &str, release: Option<&Path>) -> Child {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env("TOKS_TEST_LIFECYCLE_ROLE", role)
            .env("TOKS_TEST_LIFECYCLE_ROOT", self.root.path())
            .env("HOME", self.root.path().join("home"))
            .env("XDG_DATA_HOME", self.root.path().join("data"))
            .env("XDG_CONFIG_HOME", self.root.path().join("config-home"))
            .env("CODEX_HOME", self.root.path().join("codex"));
        if let Some(release) = release {
            command.env("TOKS_TEST_LIFECYCLE_RELEASE", release);
        }
        command.spawn().unwrap()
    }

    fn enabled(&self) -> bool {
        crate::rotation::RotationSettingsStore::for_data_dir(self.root.path().join("data/toks"))
            .load()
            .unwrap()
            .enabled()
    }

    fn set_enabled(&self, enabled: bool) {
        crate::rotation::RotationSettingsStore::for_data_dir(self.root.path().join("data/toks"))
            .update(|settings| crate::StoreUpdate::from_changed((), settings.set_enabled(enabled)))
            .unwrap();
    }

    fn attempted(&self, role: &str) -> PathBuf {
        self.root.path().join(format!("{role}-attempted"))
    }

    fn entered(&self, role: &str) -> PathBuf {
        self.root.path().join(format!("{role}-entered"))
    }

    fn service(&self) -> PathBuf {
        self.root.path().join("service")
    }

    fn config(&self) -> PathBuf {
        self.root.path().join("config")
    }

    fn updater_install(&self) -> PathBuf {
        self.root.path().join("updater-install")
    }

    fn lock(&self) -> PathBuf {
        self.root.path().join("lifecycle.lock")
    }

    fn publish(&self, candidate: &str) {
        let link = self.root.path().join("installed-router");
        remove(link.clone());
        std::os::unix::fs::symlink(self.root.path().join(candidate), link).unwrap();
    }
}

fn assert_blocked(child: &mut Child, entered: &Path) {
    thread::sleep(Duration::from_millis(100));
    assert!(!entered.exists());
    assert!(child.try_wait().unwrap().is_none());
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

fn remove(path: PathBuf) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("removing lifecycle fixture: {error}"),
    }
}
