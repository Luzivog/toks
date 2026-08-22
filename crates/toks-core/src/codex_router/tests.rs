use std::fs;

use tempfile::tempdir;

use super::codex_config::{configure_at, restore_at};
use super::systemd::{healthy_response, render_unit};
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
fn service_restarts_with_an_explicit_codex_binary_and_workspace_access() {
    let unit = render_unit(
        std::path::Path::new("/opt/Toks App/toks-router"),
        std::path::Path::new("/home/me/.local/bin/codex"),
    );
    assert!(unit.contains("Restart=on-failure"));
    assert!(unit.contains("ProtectSystem=full"));
    assert!(!unit.contains("ProtectHome="));
    assert!(unit.contains("TOKS_CODEX_BIN=/home/me/.local/bin/codex"));
    assert!(unit.contains("ExecStart=\"/opt/Toks App/toks-router\""));
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
