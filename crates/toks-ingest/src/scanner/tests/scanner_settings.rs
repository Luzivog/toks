use super::*;

#[test]
fn test_scanner_settings_deserialize_from_json_camel_case() {
    // This is the contract the CLI's settings.json relies on: the
    // field is `opencodeDbPaths`, and an empty object or missing key
    // must round-trip to Default without erroring.
    let json = r#"{
        "opencodeDbPaths": ["/one/opencode.db", "/two/opencode-stable.db"]
    }"#;
    let parsed: ScannerSettings = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.opencode_db_paths.len(), 2);
    assert_eq!(
        parsed.opencode_db_paths[0],
        PathBuf::from("/one/opencode.db")
    );
    assert_eq!(
        parsed.opencode_db_paths[1],
        PathBuf::from("/two/opencode-stable.db")
    );

    let empty: ScannerSettings = serde_json::from_str("{}").unwrap();
    assert!(empty.opencode_db_paths.is_empty());
}

#[test]
fn test_scanner_settings_deserialize_extra_scan_paths_camel_case() {
    let json = r#"{
        "extraScanPaths": {
            "codex": [
                "/tmp/project-a/.codex/sessions",
                "/tmp/project-b/.codex/archived_sessions"
            ],
            "gemini": ["/tmp/imports/gemini/tmp"]
        }
    }"#;

    let parsed: ScannerSettings = serde_json::from_str(json).unwrap();
    let serialized = serde_json::to_value(&parsed).unwrap();

    assert_eq!(
        serialized["extraScanPaths"]["codex"][0],
        serde_json::json!("/tmp/project-a/.codex/sessions")
    );
    assert_eq!(
        serialized["extraScanPaths"]["codex"][1],
        serde_json::json!("/tmp/project-b/.codex/archived_sessions")
    );
    assert_eq!(
        serialized["extraScanPaths"]["gemini"][0],
        serde_json::json!("/tmp/imports/gemini/tmp")
    );
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_merges_user_path() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    // Auto-discoverable channel db inside the default data dir.
    let data_dir = home.join(".local/share/opencode");
    fs::create_dir_all(&data_dir).unwrap();
    File::create(data_dir.join("opencode-stable.db")).unwrap();

    // User-configured db living outside XDG_DATA_HOME, the way an
    // `OPENCODE_DB=/abs/path/opencode.db` user would have it.
    let outside_dir = home.join("elsewhere");
    fs::create_dir_all(&outside_dir).unwrap();
    let outside_db = outside_dir.join("opencode.db");
    File::create(&outside_db).unwrap();

    let settings = ScannerSettings {
        opencode_db_paths: vec![outside_db.clone()],
        ..Default::default()
    };
    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["opencode".to_string()],
        false,
        &settings,
    );

    // Both paths must appear — the auto-discovered stable db and the
    // user-configured outside-XDG db.
    let names: Vec<String> = result
        .opencode_dbs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|n| n == "opencode-stable.db"),
        "expected auto-discovered opencode-stable.db, got {names:?}"
    );
    assert!(
        result.opencode_dbs.iter().any(|p| p == &outside_db),
        "expected user-configured {} in {:?}",
        outside_db.display(),
        result.opencode_dbs
    );
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_merges_settings_extra_paths() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let default_root = home.join(".codex/sessions");
    fs::create_dir_all(&default_root).unwrap();
    File::create(default_root.join("default.jsonl")).unwrap();

    let extra_root = home.join("workspace/project-a/.codex/sessions");
    fs::create_dir_all(&extra_root).unwrap();
    File::create(extra_root.join("extra.jsonl")).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "codex": [extra_root]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["codex".to_string()],
        false,
        &settings,
    );

    assert_eq!(result.get(ClientId::Codex).len(), 2);
}

#[test]
fn test_scan_all_clients_with_scanner_settings_discovers_devin_cli_extra_databases() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let default_db = home.join(".local/share/devin/cli/sessions.db");
    fs::create_dir_all(default_db.parent().unwrap()).unwrap();
    File::create(&default_db).unwrap();

    let extra_root = home.join("imports/devin");
    let extra_db = extra_root.join("profile/sessions.db");
    fs::create_dir_all(extra_db.parent().unwrap()).unwrap();
    File::create(&extra_db).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "devin-cli": [extra_root]
        }
    }))
    .unwrap();
    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["devin-cli".to_string()],
        false,
        &settings,
    );

    assert_eq!(result.devin_dbs, vec![default_db, extra_db]);
    assert!(
        result.get(ClientId::DevinCli).is_empty(),
        "Devin SQLite databases should use the dedicated scan result"
    );
}

#[test]
fn test_devin_desktop_scan_includes_configured_cli_lookup_databases() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let extra_root = home.join("imports/devin");
    let extra_db = extra_root.join("profile/sessions.db");
    fs::create_dir_all(extra_db.parent().unwrap()).unwrap();
    File::create(&extra_db).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "devin-cli": [extra_root]
        }
    }))
    .unwrap();
    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["devin-desktop".to_string()],
        false,
        &settings,
    );

    assert_eq!(result.devin_dbs, vec![extra_db]);
}
