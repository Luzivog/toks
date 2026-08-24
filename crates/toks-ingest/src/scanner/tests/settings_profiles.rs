use super::*;

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_merges_hermes_extra_profile_db() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let default_dir = home.join(".hermes");
    fs::create_dir_all(&default_dir).unwrap();
    let default_db = default_dir.join("state.db");
    File::create(&default_db).unwrap();

    let profile_dir = home.join(".hermes/profiles/director_planning");
    fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    File::create(&profile_db).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "hermes": [
                profile_dir,
                profile_db
            ]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        false,
        &settings,
    );

    assert_eq!(result.hermes_db.as_ref(), Some(&default_db));
    assert_eq!(result.hermes_db_paths(), vec![default_db, profile_db]);
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_auto_discovers_hermes_profile_dbs() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let default_dir = home.join(".hermes");
    fs::create_dir_all(&default_dir).unwrap();
    let default_db = default_dir.join("state.db");
    File::create(&default_db).unwrap();

    let profile_a_dir = home.join(".hermes/profiles/director_planning");
    fs::create_dir_all(&profile_a_dir).unwrap();
    let profile_a_db = profile_a_dir.join("state.db");
    File::create(&profile_a_db).unwrap();

    let profile_b_dir = home.join(".hermes/profiles/research");
    fs::create_dir_all(&profile_b_dir).unwrap();
    let profile_b_db = profile_b_dir.join("state.db");
    File::create(&profile_b_db).unwrap();

    // Shallow discovery should not pick up arbitrary nested state.db files.
    let nested_dir = home.join(".hermes/profiles/research/archive");
    fs::create_dir_all(&nested_dir).unwrap();
    File::create(nested_dir.join("state.db")).unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        false,
        &ScannerSettings::default(),
    );

    assert_eq!(result.hermes_db.as_ref(), Some(&default_db));
    assert_eq!(
        result.hermes_db_paths(),
        vec![default_db, profile_a_db, profile_b_db]
    );
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_auto_discovers_hermes_profiles_without_default_db() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let profile_dir = home.join(".hermes/profiles/research");
    fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    File::create(&profile_db).unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        false,
        &ScannerSettings::default(),
    );

    assert_eq!(result.hermes_db, None);
    assert_eq!(result.hermes_db_paths(), vec![profile_db]);
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_auto_discovers_hermes_profiles_under_env_home() {
    let mut env = EnvGuard::capture(&["HERMES_HOME", "TOKSCOPE_EXTRA_DIRS"]);
    env.remove("TOKSCOPE_EXTRA_DIRS");
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let hermes_home = home.join("custom-hermes-home");

    fs::create_dir_all(&hermes_home).unwrap();
    let default_db = hermes_home.join("state.db");
    File::create(&default_db).unwrap();

    let profile_dir = hermes_home.join("profiles/research");
    fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    File::create(&profile_db).unwrap();

    env.set("HERMES_HOME", &hermes_home);
    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        true,
        &ScannerSettings::default(),
    );

    assert_eq!(result.hermes_db.as_ref(), Some(&default_db));
    assert_eq!(result.hermes_db_paths(), vec![default_db, profile_db]);
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_profile_scoped_hermes_home_isolates_to_own_profile()
{
    // Data-isolation guarantee: a profile-scoped `HERMES_HOME` must NOT pull
    // in sibling profiles under `<root>/profiles/*` or the default profile at
    // `<root>/state.db`. Only the scoped profile's own `state.db` is scanned.
    let mut env = EnvGuard::capture(&["HERMES_HOME", "TOKSCOPE_EXTRA_DIRS"]);
    env.remove("TOKSCOPE_EXTRA_DIRS");
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let profile_root = home.join(".hermes/profiles");
    let default_db = home.join(".hermes/state.db");
    fs::create_dir_all(default_db.parent().unwrap()).unwrap();
    File::create(&default_db).unwrap();

    let coder_dir = profile_root.join("coder");
    fs::create_dir_all(&coder_dir).unwrap();
    let coder_db = coder_dir.join("state.db");
    File::create(&coder_db).unwrap();

    let research_dir = profile_root.join("research");
    fs::create_dir_all(&research_dir).unwrap();
    let research_db = research_dir.join("state.db");
    File::create(&research_db).unwrap();

    // Profile-scoped homes must also not scan `<active-profile>/profiles`.
    let nested_dir = coder_dir.join("profiles/archived");
    fs::create_dir_all(&nested_dir).unwrap();
    File::create(nested_dir.join("state.db")).unwrap();

    env.set("HERMES_HOME", &coder_dir);
    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        true,
        &ScannerSettings::default(),
    );

    assert_eq!(result.hermes_db.as_ref(), Some(&coder_db));
    assert_eq!(result.hermes_db_paths(), vec![coder_db.clone()]);
    assert!(
        !result.hermes_db_paths().contains(&research_db),
        "profile-scoped HERMES_HOME must not discover sibling profiles"
    );
    assert!(
        !result.hermes_db_paths().contains(&default_db),
        "profile-scoped HERMES_HOME must not discover the default profile"
    );
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_discovers_hermes_windows_local_appdata_home() {
    // Native Windows root: Hermes stores its home under
    // `%LOCALAPPDATA%\hermes` (literal `<home>/AppData/Local/hermes`). Run
    // with env roots disabled so this exercises the cross-platform
    // `AppData/Local` fallback, mirroring the Crush LOCALAPPDATA tests.
    let previous_hermes_home = std::env::var("HERMES_HOME").ok();
    let previous_local_app_data = std::env::var("LOCALAPPDATA").ok();
    unsafe { std::env::remove_var("HERMES_HOME") };
    unsafe { std::env::remove_var("LOCALAPPDATA") };

    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let windows_home = home.join("AppData/Local/hermes");
    fs::create_dir_all(&windows_home).unwrap();
    let default_db = windows_home.join("state.db");
    File::create(&default_db).unwrap();

    let profile_dir = windows_home.join("profiles/research");
    fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    File::create(&profile_db).unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        false,
        &ScannerSettings::default(),
    );

    restore_env("HERMES_HOME", previous_hermes_home);
    restore_env("LOCALAPPDATA", previous_local_app_data);

    assert_eq!(result.hermes_db.as_ref(), Some(&default_db));
    assert_eq!(result.hermes_db_paths(), vec![default_db, profile_db]);
}

#[test]
fn test_scan_all_clients_with_scanner_settings_discovers_zed_windows_local_appdata_home() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let windows_threads_dir = home.join("AppData/Local/Zed/threads");
    fs::create_dir_all(&windows_threads_dir).unwrap();
    let threads_db = windows_threads_dir.join("threads.db");
    File::create(&threads_db).unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["zed".to_string()],
        false,
        &ScannerSettings::default(),
    );

    assert_eq!(result.zed_db.as_ref(), Some(&threads_db));
}

#[test]
fn test_scan_all_clients_with_scanner_settings_merges_zed_extra_threads_db() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let windows_threads_dir = home.join("AppData/Local/Zed/threads");
    fs::create_dir_all(&windows_threads_dir).unwrap();
    let threads_db = windows_threads_dir.join("threads.db");
    File::create(&threads_db).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "zed": [windows_threads_dir]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["zed".to_string()],
        false,
        &settings,
    );

    assert_eq!(result.zed_db_paths(), vec![threads_db]);
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_respects_hermes_client_filter() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let profile_dir = home.join(".hermes/profiles/director_planning");
    fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    File::create(&profile_db).unwrap();

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "hermes": [profile_dir]
        }
    }))
    .unwrap();

    let claude_only = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["claude".to_string()],
        true,
        &settings,
    );
    assert!(claude_only.hermes_db_paths().is_empty());

    let hermes_only = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["hermes".to_string()],
        false,
        &settings,
    );
    assert_eq!(hermes_only.hermes_db_paths(), vec![profile_db]);
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_dedups_settings_and_env_extra_paths() {
    let mut env =
        EnvGuard::capture(&["TOKSCOPE_EXTRA_DIRS", "TOKSCOPE_HEADLESS_DIR", "CODEX_HOME"]);
    env.remove("TOKSCOPE_HEADLESS_DIR");
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    env.set("CODEX_HOME", home.join(".codex"));

    let default_root = home.join(".codex/sessions");
    fs::create_dir_all(&default_root).unwrap();
    File::create(default_root.join("default.jsonl")).unwrap();

    let extra_root = home.join("workspace/project-a/.codex/sessions");
    fs::create_dir_all(&extra_root).unwrap();
    File::create(extra_root.join("extra.jsonl")).unwrap();

    env.set(
        "TOKSCOPE_EXTRA_DIRS",
        format!("codex:{}", extra_root.join("..").join("sessions").display()),
    );

    let settings: ScannerSettings = serde_json::from_value(serde_json::json!({
        "extraScanPaths": {
            "codex": [extra_root]
        }
    }))
    .unwrap();

    let result = scan_all_clients_with_scanner_settings(
        home.to_str().unwrap(),
        &["codex".to_string()],
        true,
        &settings,
    );

    assert_eq!(result.get(ClientId::Codex).len(), 2);
}

#[test]
#[serial]
fn test_scan_all_clients_with_scanner_settings_respects_opencode_client_filter() {
    // Regression guard: previously the scanner unconditionally
    // merged `scanner.opencodeDbPaths` after the inner scan, which
    // bypassed the existing `enabled.contains(&ClientId::OpenCode)`
    // guard. A request like `tokscope --claude` would still pull in
    // user-pinned OpenCode dbs and inflate `parse_local_clients`
    // counts plus waste SQLite parsing work.
    //
    // The fix moves the merge inside the OpenCode-enabled block, so
    // this test exercises the four canonical filter shapes:
    //   1. ["claude"]    → opencode_dbs must be empty
    //   2. ["opencode"]  → both auto + user-configured dbs present
    //   3. ["synthetic"] → both present (synthetic enables all)
    //   4. []            → both present (empty filter = all clients)
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    // Auto-discoverable channel db inside XDG data dir.
    let data_dir = home.join(".local/share/opencode");
    fs::create_dir_all(&data_dir).unwrap();
    let auto_db = data_dir.join("opencode.db");
    File::create(&auto_db).unwrap();

    // User-configured db living outside XDG_DATA_HOME (mirrors the
    // `OPENCODE_DB=/abs/path/opencode.db` use case).
    let outside_dir = home.join("elsewhere");
    fs::create_dir_all(&outside_dir).unwrap();
    let outside_db = outside_dir.join("opencode.db");
    File::create(&outside_db).unwrap();

    let settings = ScannerSettings {
        opencode_db_paths: vec![outside_db.clone()],
        ..Default::default()
    };

    let scan = |clients: &[&str]| {
        let owned: Vec<String> = clients.iter().map(|s| s.to_string()).collect();
        scan_all_clients_with_scanner_settings(home.to_str().unwrap(), &owned, false, &settings)
    };

    // 1. clients=["claude"] — OpenCode disabled, dbs must stay empty.
    let claude_only = scan(&["claude"]);
    assert!(
        claude_only.opencode_dbs.is_empty(),
        "scanner.opencodeDbPaths must NOT leak into a Claude-only scan, \
         got {:?}",
        claude_only.opencode_dbs
    );

    // 2. clients=["opencode"] — both auto-discovered + user-configured.
    let opencode_only = scan(&["opencode"]);
    assert!(
        opencode_only.opencode_dbs.iter().any(|p| p == &auto_db),
        "expected auto-discovered {} in {:?}",
        auto_db.display(),
        opencode_only.opencode_dbs
    );
    assert!(
        opencode_only.opencode_dbs.iter().any(|p| p == &outside_db),
        "expected user-configured {} in {:?}",
        outside_db.display(),
        opencode_only.opencode_dbs
    );

    // 3. clients=["synthetic"] — synthetic enables all clients, so
    //    both dbs must be present.
    let synthetic_only = scan(&["synthetic"]);
    assert!(
        synthetic_only.opencode_dbs.iter().any(|p| p == &auto_db),
        "synthetic-only filter must enable OpenCode auto-discovery, got {:?}",
        synthetic_only.opencode_dbs
    );
    assert!(
        synthetic_only.opencode_dbs.iter().any(|p| p == &outside_db),
        "synthetic-only filter must merge user-configured paths, got {:?}",
        synthetic_only.opencode_dbs
    );

    // 4. clients=[] — empty filter = all clients = both dbs present.
    let all_clients = scan(&[]);
    assert!(
        all_clients.opencode_dbs.iter().any(|p| p == &auto_db),
        "empty client filter must enable OpenCode auto-discovery, got {:?}",
        all_clients.opencode_dbs
    );
    assert!(
        all_clients.opencode_dbs.iter().any(|p| p == &outside_db),
        "empty client filter must merge user-configured paths, got {:?}",
        all_clients.opencode_dbs
    );
}
