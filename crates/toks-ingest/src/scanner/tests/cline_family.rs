use super::*;

#[test]
fn test_scan_all_clients_roocode() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_roocode_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["roocode".to_string()], false);
    assert_eq!(result.get(ClientId::RooCode).len(), 2);
    assert!(result
        .get(ClientId::RooCode)
        .iter()
        .all(|p| p.ends_with("ui_messages.json")));
}

#[test]
fn test_scan_all_clients_kilocode() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_kilocode_dir(home);

    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["kilocode".to_string()],
        false,
    );
    assert_eq!(result.get(ClientId::KiloCode).len(), 2);
    assert!(result
        .get(ClientId::KiloCode)
        .iter()
        .all(|p| p.ends_with("ui_messages.json")));
}

#[test]
fn test_scan_all_clients_cline() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_cline_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], false);
    assert_eq!(result.get(ClientId::Cline).len(), 4);
    assert!(result
        .get(ClientId::Cline)
        .iter()
        .all(|p| p.ends_with("ui_messages.json")));
}

#[test]
fn test_scan_all_clients_cline_cli() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_cline_cli_dir(&home.join(".cline/data"));

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], false);
    assert_eq!(result.get(ClientId::Cline).len(), 1);
    assert!(result.get(ClientId::Cline)[0]
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".messages.json")));
}

#[test]
#[serial]
fn test_scan_all_clients_cline_cli_session_data_dir_takes_precedence() {
    let mut env = EnvGuard::capture(&["CLINE_SESSION_DATA_DIR", "CLINE_DATA_DIR", "CLINE_DIR"]);
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let session_data_dir = dir.path().join("custom-cline-sessions");
    let data_dir = dir.path().join("custom-cline-data");
    let cline_dir = dir.path().join("custom-cline");

    setup_mock_cline_cli_session_root(&session_data_dir);
    setup_mock_cline_cli_dir(&data_dir);
    setup_mock_cline_cli_dir(&cline_dir.join("data"));
    setup_mock_cline_cli_dir(&home.join(".cline/data"));
    env.set("CLINE_SESSION_DATA_DIR", &session_data_dir);
    env.set("CLINE_DATA_DIR", &data_dir);
    env.set("CLINE_DIR", &cline_dir);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], true);
    let expected = session_data_dir.join("cli-session/cli-session.messages.json");

    assert_eq!(result.get(ClientId::Cline), &vec![expected]);
}

#[test]
#[serial]
fn test_scan_all_clients_cline_cli_uses_data_dir_override() {
    let mut env = EnvGuard::capture(&["CLINE_SESSION_DATA_DIR", "CLINE_DATA_DIR", "CLINE_DIR"]);
    env.remove("CLINE_SESSION_DATA_DIR");
    env.remove("CLINE_DIR");
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let data_dir = dir.path().join("custom-cline-data");
    let cline_dir = dir.path().join("custom-cline");

    setup_mock_cline_cli_dir(&data_dir);
    setup_mock_cline_cli_dir(&cline_dir.join("data"));
    setup_mock_cline_cli_dir(&home.join(".cline/data"));
    env.set("CLINE_DATA_DIR", &data_dir);
    env.set("CLINE_DIR", &cline_dir);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], true);
    let expected = data_dir.join("sessions/cli-session/cli-session.messages.json");

    assert_eq!(result.get(ClientId::Cline), &vec![expected]);
}

#[test]
#[serial]
fn test_scan_all_clients_cline_cli_uses_cline_dir_override() {
    let mut env = EnvGuard::capture(&["CLINE_SESSION_DATA_DIR", "CLINE_DATA_DIR", "CLINE_DIR"]);
    env.remove("CLINE_SESSION_DATA_DIR");
    env.remove("CLINE_DATA_DIR");
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let cline_dir = dir.path().join("custom-cline");

    setup_mock_cline_cli_dir(&cline_dir.join("data"));
    setup_mock_cline_cli_dir(&home.join(".cline/data"));
    env.set("CLINE_DIR", &cline_dir);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], true);
    let expected = cline_dir.join("data/sessions/cli-session/cli-session.messages.json");

    assert_eq!(result.get(ClientId::Cline), &vec![expected]);
}

#[test]
#[serial]
fn test_scan_all_clients_cline_cli_ignores_env_roots_when_disabled() {
    let mut env = EnvGuard::capture(&["CLINE_SESSION_DATA_DIR", "CLINE_DATA_DIR", "CLINE_DIR"]);
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let session_data_dir = dir.path().join("custom-cline-sessions");
    let data_dir = dir.path().join("custom-cline-data");
    let cline_dir = dir.path().join("custom-cline");

    setup_mock_cline_cli_session_root(&session_data_dir);
    setup_mock_cline_cli_dir(&data_dir);
    setup_mock_cline_cli_dir(&cline_dir.join("data"));
    setup_mock_cline_cli_dir(&home.join(".cline/data"));
    env.set("CLINE_SESSION_DATA_DIR", &session_data_dir);
    env.set("CLINE_DATA_DIR", &data_dir);
    env.set("CLINE_DIR", &cline_dir);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], false);
    let expected = home.join(".cline/data/sessions/cli-session/cli-session.messages.json");

    assert_eq!(result.get(ClientId::Cline), &vec![expected]);
}

#[test]
#[serial]
fn test_scan_all_clients_cline_cli_whitespace_data_dir_uses_default() {
    let mut env = EnvGuard::capture(&["CLINE_SESSION_DATA_DIR", "CLINE_DATA_DIR", "CLINE_DIR"]);
    env.remove("CLINE_SESSION_DATA_DIR");
    env.remove("CLINE_DIR");
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_cline_cli_dir(&home.join(".cline/data"));
    env.set("CLINE_DATA_DIR", " \t ");

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["cline".to_string()], true);
    let expected = home.join(".cline/data/sessions/cli-session/cli-session.messages.json");

    assert_eq!(result.get(ClientId::Cline), &vec![expected]);
}
