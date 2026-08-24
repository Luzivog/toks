use super::*;

#[test]
#[serial]
fn test_grok_extra_scan_path_discovers_both_sources() {
    // Regression guard: `scanner.extraScanPaths.grok` roots must receive
    // the same dual-source discovery as the primary Grok home — legacy
    // `updates.jsonl` under the configured root AND the sibling
    // `logs/unified.jsonl` derived from the Grok home. Previously the
    // unified log was only added for the resolved primary home, so an
    // alternate root contributed only the registered `updates.jsonl`
    // pattern and its inference breakdowns were silently missed.
    let mut env = EnvGuard::capture(&["GROK_HOME", "TOKSCOPE_EXTRA_DIRS"]);
    env.remove("GROK_HOME");
    env.remove("TOKSCOPE_EXTRA_DIRS");

    let home = TempDir::new().unwrap();
    let alt_home = TempDir::new().unwrap();

    // Alternate Grok home laid out like a real ~/.grok.
    let alt_session = alt_home
        .path()
        .join("sessions/%2Ftmp%2Fproject/session-alt");
    fs::create_dir_all(&alt_session).unwrap();
    File::create(alt_session.join("updates.jsonl")).unwrap();
    // A nested legacy update NOT under sessions/ — the configured root is
    // a recursive scan root, so nested updates.jsonl must keep matching
    // instead of being replaced by a sessions/ subdirectory task.
    let nested = alt_home.path().join("imports/nested");
    fs::create_dir_all(&nested).unwrap();
    File::create(nested.join("updates.jsonl")).unwrap();
    fs::create_dir_all(alt_home.path().join("logs")).unwrap();
    File::create(alt_home.path().join("logs/unified.jsonl")).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "grok": [alt_home.path()]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["grok".to_string()],
        true,
        &settings,
    );

    let files = result.get(ClientId::Grok);
    assert_eq!(
        files
            .iter()
            .filter(|p| p.ends_with("updates.jsonl"))
            .count(),
        2,
        "alternate Grok root must keep recursive updates.jsonl discovery: {files:?}"
    );
    assert!(
        files.iter().any(|p| p.ends_with("unified.jsonl")),
        "alternate Grok root must contribute logs/unified.jsonl: {files:?}"
    );
}

#[test]
#[serial]
fn test_grok_extra_scan_path_sessions_shape_discovers_unified_log() {
    // extraScanPaths.grok may point at the `sessions` subdirectory (the
    // shape the primary resolution returns) instead of the home itself;
    // the unified log is derived from the parent home either way.
    let mut env = EnvGuard::capture(&["GROK_HOME", "TOKSCOPE_EXTRA_DIRS"]);
    env.remove("GROK_HOME");
    env.remove("TOKSCOPE_EXTRA_DIRS");

    let home = TempDir::new().unwrap();
    let alt_home = TempDir::new().unwrap();

    let alt_session = alt_home
        .path()
        .join("sessions/%2Ftmp%2Fproject/session-alt");
    fs::create_dir_all(&alt_session).unwrap();
    File::create(alt_session.join("updates.jsonl")).unwrap();
    fs::create_dir_all(alt_home.path().join("logs")).unwrap();
    File::create(alt_home.path().join("logs/unified.jsonl")).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "grok": [alt_home.path().join("sessions")]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["grok".to_string()],
        true,
        &settings,
    );

    let files = result.get(ClientId::Grok);
    assert_eq!(
        files.len(),
        2,
        "expected updates.jsonl + unified.jsonl: {files:?}"
    );
    assert!(files.iter().any(|p| p.ends_with("updates.jsonl")));
    assert!(files.iter().any(|p| p.ends_with("unified.jsonl")));
}

#[test]
#[serial]
fn test_grok_extra_scan_path_nested_session_shape_discovers_unified_log() {
    // A root below a mixed-case `sessions` directory still belongs to the
    // surrounding Grok home, so its sibling logs/unified.jsonl is found.
    let mut env = EnvGuard::capture(&["GROK_HOME", "TOKSCOPE_EXTRA_DIRS"]);
    env.remove("GROK_HOME");
    env.remove("TOKSCOPE_EXTRA_DIRS");

    let home = TempDir::new().unwrap();
    let alt_home = TempDir::new().unwrap();

    let alt_session = alt_home
        .path()
        .join("Sessions/%2Ftmp%2Fproject/session-alt");
    fs::create_dir_all(&alt_session).unwrap();
    File::create(alt_session.join("updates.jsonl")).unwrap();
    fs::create_dir_all(alt_home.path().join("logs")).unwrap();
    File::create(alt_home.path().join("logs/unified.jsonl")).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "grok": [alt_session]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["grok".to_string()],
        true,
        &settings,
    );

    let files = result.get(ClientId::Grok);
    assert_eq!(
        files.len(),
        2,
        "expected updates.jsonl + unified.jsonl: {files:?}"
    );
    assert!(files.iter().any(|p| p.ends_with("updates.jsonl")));
    assert!(files.iter().any(|p| p.ends_with("unified.jsonl")));
}

#[test]
fn test_scan_all_clients_grok() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_grok_dir(home);

    let unified_dir = home.join(".grok/logs");
    fs::create_dir_all(&unified_dir).unwrap();
    let unified_log = unified_dir.join("unified.jsonl");
    File::create(&unified_log).unwrap();
    let nested_dir = unified_dir.join("archive");
    fs::create_dir_all(&nested_dir).unwrap();
    let nested_log = nested_dir.join("unified.jsonl");
    File::create(&nested_log).unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["grok".to_string()], false);
    let grok_files = result.get(ClientId::Grok);
    assert_eq!(grok_files.len(), 2);
    assert!(grok_files
        .iter()
        .any(|path| path.ends_with("updates.jsonl")));
    assert!(grok_files.iter().any(|path| path == &unified_log));
    assert!(!grok_files.iter().any(|path| path == &nested_log));
    assert!(result.get(ClientId::OpenCode).is_empty());
    assert!(result.get(ClientId::Claude).is_empty());
}

#[test]
fn test_scan_all_clients_jcode() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_jcode_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["jcode".to_string()], false);
    assert_eq!(result.get(ClientId::Jcode).len(), 1);
    assert!(result.get(ClientId::Jcode)[0].ends_with("session_fixture.json"));
    assert!(result.get(ClientId::OpenCode).is_empty());
    assert!(result.get(ClientId::Claude).is_empty());
}
