use std::fs;

use tempfile::tempdir;

use super::codex_config::{configure_at, restore_at};
use super::host::BuildId;
use super::systemd::{
    healthy_response, render_service_unit, render_socket_unit, render_worker_unit, UnitEnvironment,
    COORDINATOR_STOP_TIMEOUT_SECONDS,
};
use super::ROUTER_BASE_URL;

#[test]
fn configuration_round_trip_restores_existing_value() {
    let root = tempdir().unwrap();
    let config = root.path().join("codex/config.toml");
    let backup = root.path().join("toks/backup.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(
        &config,
        "model = \"gpt-5.6-sol\"\nopenai_base_url = \"http://old.example\"\n",
    )
    .unwrap();

    configure_at(&config, &backup).unwrap();
    let configured = fs::read_to_string(&config).unwrap();
    assert!(configured.contains(ROUTER_BASE_URL));
    configure_at(&config, &backup).unwrap();

    restore_at(&config, &backup).unwrap();
    let restored = fs::read_to_string(&config).unwrap();
    assert!(restored.contains("http://old.example"));
    assert!(!backup.exists());
}

#[test]
fn restore_does_not_overwrite_a_later_user_change() {
    let root = tempdir().unwrap();
    let config = root.path().join("config.toml");
    let backup = root.path().join("backup.json");
    configure_at(&config, &backup).unwrap();
    fs::write(&config, "openai_base_url = \"http://new.example\"\n").unwrap();

    restore_at(&config, &backup).unwrap();

    assert!(fs::read_to_string(config)
        .unwrap()
        .contains("http://new.example"));
}

#[test]
fn a_fresh_activation_replaces_a_stale_backup() {
    let root = tempdir().unwrap();
    let config = root.path().join("config.toml");
    let backup = root.path().join("backup.json");
    configure_at(&config, &backup).unwrap();
    fs::write(&config, "openai_base_url = \"http://new.example\"\n").unwrap();

    configure_at(&config, &backup).unwrap();
    restore_at(&config, &backup).unwrap();

    assert!(fs::read_to_string(config)
        .unwrap()
        .contains("http://new.example"));
}

#[test]
fn systemd_owns_the_listener_and_restarts_only_the_coordinator() {
    let environment = UnitEnvironment::from_values([
        Some("/custom/bin"),
        None,
        Some("/home/me/.local/share"),
        None,
    ]);
    let build = BuildId::new("baked-build").unwrap();
    let service = render_service_unit(
        std::path::Path::new("/opt/Toks App/toks-router"),
        std::path::Path::new("/home/me/.local/bin/codex"),
        &build,
        &environment,
    )
    .unwrap();
    assert!(service.contains("Restart=always"));
    assert!(service.contains("Sockets=toks-router.socket"));
    // A mount namespace denies the /proc reads the coordinator and its workers
    // use to attest each other, so no filesystem-namespacing hardening here.
    assert!(!service.contains("ProtectSystem="));
    assert!(!service.contains("ProtectHome="));
    assert!(service.contains("TOKS_CODEX_BIN=/home/me/.local/bin/codex"));
    assert!(service.contains("ExecStart=\"/opt/Toks App/toks-router\" launch-host"));
    assert!(service.contains("RuntimeDirectory=toks-router"));
    assert!(service.contains("RuntimeDirectoryPreserve=restart"));
    assert!(service.contains("TOKS_ROUTER_BUILD_ID=baked-build"));
    assert!(service.contains("Environment=\"PATH=/custom/bin\""));
    assert!(service.contains("UnsetEnvironment=CODEX_HOME"));
    assert!(service.contains("KillMode=control-group"));
    assert!(service.contains("TimeoutStopSec=30s"));
    assert!(!service.contains("TimeoutStopSec=infinity"));
    assert!(
        COORDINATOR_STOP_TIMEOUT_SECONDS
            > super::host::COORDINATOR_PRE_SIGNAL_OPERATION_TIMEOUT.as_secs()
                + super::host::COORDINATOR_SHUTDOWN_DRAIN_TIMEOUT.as_secs()
    );

    let socket = render_socket_unit();
    assert!(socket.contains("ListenStream=127.0.0.1:47837"));
    assert!(socket.contains("FileDescriptorName=router"));
    assert!(socket.contains("SocketMode=0600"));
    assert!(socket.contains("Accept=no"));

    let worker = render_worker_unit(std::path::Path::new("/opt/Toks App/router")).unwrap();
    assert!(
        worker.contains("ExecStart=\"/opt/Toks App/router/generations/%i/toks-router\" launch-worker %i \"/opt/Toks App/router/generations/%i/launch.json\"")
    );
    assert!(!worker.contains("TOKS_CODEX_BIN="));
    // Namespacing the worker would deny it the coordinator's /proc entries, so
    // it could never authorize the handoff channel.
    assert!(!worker.contains("ProtectSystem="));
    assert!(worker.contains("TimeoutStopSec=15s"));
    assert!(!worker.contains("TimeoutStopSec=infinity"));
    assert!(!worker.contains("PartOf=toks-router.service"));
    assert!(!worker.contains("BindsTo=toks-router.service"));
    assert!(!worker.contains("Requires=toks-router.service"));
}

#[test]
fn readiness_rejects_an_unrelated_service_on_the_router_port() {
    assert!(healthy_response(
        b"HTTP/1.1 200 OK\r\ncontent-length: 12\r\n\r\ntoks-router\n"
    ));
    assert!(!healthy_response(
        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nOK"
    ));
}

#[test]
fn reset_acknowledgement_updates_the_process_safe_runtime_store() {
    use std::collections::BTreeMap;

    use crate::accounts::AccountId;
    use crate::rotation::{
        BlockWindow, FastLimitDisposition, QuotaObservation, RotationRuntime, RotationRuntimeStore,
        ThreadId, UnixMillis,
    };

    let directory = tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let account = AccountId::new("account");
    let thread = ThreadId::new("thread");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &thread).unwrap();
    runtime.apply_quota_observations(
        &BTreeMap::from([(
            account.clone(),
            QuotaObservation::Draining(Some(UnixMillis::new(100))),
        )]),
        UnixMillis::new(1),
    );
    runtime.fast_limit_reached(
        &account,
        &thread,
        BlockWindow::known(UnixMillis::new(100)),
        FastLimitDisposition::RetryingStandard,
        UnixMillis::new(2),
    );
    store.save(&runtime).unwrap();

    super::acknowledge_banked_reset_in(&store, &account).unwrap();

    assert!(!store
        .load()
        .unwrap()
        .requires_standard_tier(&account, &thread, UnixMillis::new(3)));
}
