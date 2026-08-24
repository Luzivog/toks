use super::*;

#[test]
#[serial]
fn test_scan_all_clients_zed_xdg_db() {
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let zed_db = setup_mock_zed_xdg_db(home);
    unsafe { std::env::set_var("XDG_DATA_HOME", home.join(".local/share")) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["zed".to_string()]);

    assert_eq!(result.zed_db.as_ref(), Some(&zed_db));
    restore_env("XDG_DATA_HOME", previous_xdg);
}

#[cfg(target_os = "macos")]
#[test]
#[serial]
fn test_scan_all_clients_zed_macos_fallback() {
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let zed_db = setup_mock_zed_macos_db(home);
    unsafe { std::env::remove_var("XDG_DATA_HOME") };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["zed".to_string()]);

    assert_eq!(result.zed_db.as_ref(), Some(&zed_db));
    restore_env("XDG_DATA_HOME", previous_xdg);
}

#[test]
fn test_scan_all_clients_copilot() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_copilot_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["copilot".to_string()], false);

    assert_eq!(result.get(ClientId::Copilot).len(), 1);
    assert!(result.get(ClientId::Copilot)[0].ends_with("copilot.jsonl"));
}

#[test]
#[serial]
fn test_scan_all_clients_copilot_includes_explicit_exporter_file() {
    let previous = std::env::var("COPILOT_OTEL_FILE_EXPORTER_PATH").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let explicit_dir = home.join("otel-export");
    fs::create_dir_all(&explicit_dir).unwrap();
    let explicit_file = explicit_dir.join("copilot-explicit.jsonl");
    File::create(&explicit_file).unwrap();

    unsafe { std::env::set_var("COPILOT_OTEL_FILE_EXPORTER_PATH", &explicit_file) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["copilot".to_string()]);

    assert_eq!(result.get(ClientId::Copilot), &vec![explicit_file]);

    restore_env("COPILOT_OTEL_FILE_EXPORTER_PATH", previous);
}

#[test]
fn test_scan_all_clients_kiro_includes_cli_and_global_storage() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_kiro_dir(home);
    setup_mock_kiro_global_storage_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["kiro".to_string()], false);
    assert_eq!(result.get(ClientId::Kiro).len(), 4);
    assert!(result
        .get(ClientId::Kiro)
        .iter()
        .any(|p| p.ends_with("session-001.json")));
    assert!(result
        .get(ClientId::Kiro)
        .iter()
        .any(|p| p.ends_with("execution.chat")));
    assert!(result
        .get(ClientId::Kiro)
        .iter()
        .any(|p| p.ends_with("execution")));
}

#[test]
fn test_scan_all_clients_kiro_includes_ide_sessions() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let sess_dir = home.join(".kiro/sessions/workspace-a/sess_02f1c107");
    fs::create_dir_all(&sess_dir).unwrap();
    File::create(sess_dir.join("session.json")).unwrap();
    File::create(sess_dir.join("messages.jsonl")).unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["kiro".to_string()], false);
    assert!(result
        .get(ClientId::Kiro)
        .iter()
        .any(|p| p.ends_with("sess_02f1c107/session.json")));
    // The sibling messages.jsonl is read by the parser, not scanned directly.
    assert!(!result
        .get(ClientId::Kiro)
        .iter()
        .any(|p| p.ends_with("messages.jsonl")));
}
