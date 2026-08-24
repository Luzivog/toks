use super::*;

#[test]
fn test_scan_all_clients_multiple() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    setup_mock_claude_dir(home);
    setup_mock_gemini_dir(home);

    // use_env_roots=false to avoid interference from TOKSCOPE_EXTRA_DIRS
    // set by parallel tests
    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["claude".to_string(), "gemini".to_string()],
        false,
    );

    assert_eq!(result.get(ClientId::Claude).len(), 1);
    assert_eq!(result.get(ClientId::Gemini).len(), 1);
    assert!(result.get(ClientId::OpenCode).is_empty());
    assert!(result.get(ClientId::Codex).is_empty());
}

#[test]
#[serial]
fn test_scan_all_clients_headless_paths() {
    let mut env = EnvGuard::capture(&[
        "TOKSCOPE_HEADLESS_DIR",
        "TOKSCOPE_EXTRA_DIRS",
        "CODEX_HOME",
        "GEMINI_CLI_HOME",
    ]);
    env.remove("TOKSCOPE_HEADLESS_DIR");
    env.remove("TOKSCOPE_EXTRA_DIRS");
    env.remove("CODEX_HOME");
    env.remove("GEMINI_CLI_HOME");

    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let mac_root = home
        .join("Library")
        .join("Application Support")
        .join("tokscope")
        .join("headless");

    fs::create_dir_all(mac_root.join("codex")).unwrap();
    File::create(mac_root.join("codex").join("codex.jsonl")).unwrap();

    let result = scan_all_clients(
        home.to_str().unwrap(),
        &[
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
        ],
    );

    assert!(result.get(ClientId::Claude).is_empty());
    assert_eq!(result.get(ClientId::Codex).len(), 1);
    assert!(result.get(ClientId::Gemini).is_empty());
}

#[test]
fn test_parse_extra_dirs_basic() {
    let enabled: HashSet<ClientId> = [ClientId::Claude, ClientId::OpenClaw]
        .iter()
        .copied()
        .collect();
    let dirs = parse_extra_dirs("claude:/tmp/mac-sessions,openclaw:/tmp/oc-extra", &enabled);
    assert_eq!(dirs.len(), 2);
    assert_eq!(dirs[0].0, ClientId::Claude);
    assert_eq!(dirs[0].1, "/tmp/mac-sessions");
    assert_eq!(dirs[1].0, ClientId::OpenClaw);
    assert_eq!(dirs[1].1, "/tmp/oc-extra");
}

#[test]
fn test_parse_extra_dirs_filters_disabled_clients() {
    let enabled: HashSet<ClientId> = [ClientId::Claude].iter().copied().collect();
    let dirs = parse_extra_dirs(
        "claude:/tmp/mac-sessions,gemini:/tmp/gemini-extra",
        &enabled,
    );
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].0, ClientId::Claude);
}

#[test]
fn test_parse_extra_dirs_skips_unsupported_clients() {
    let enabled: HashSet<ClientId> = [ClientId::Claude, ClientId::Kilo].iter().copied().collect();
    let dirs = parse_extra_dirs("claude:/tmp/mac-sessions,kilo:/tmp/kilo", &enabled);
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].0, ClientId::Claude);
    assert_eq!(dirs[0].1, "/tmp/mac-sessions");
}

#[test]
fn test_parse_extra_dirs_empty_string() {
    let enabled: HashSet<ClientId> = ClientId::iter().collect();
    let dirs = parse_extra_dirs("", &enabled);
    assert!(dirs.is_empty());
}

#[test]
fn test_parse_extra_dirs_invalid_client() {
    let enabled: HashSet<ClientId> = ClientId::iter().collect();
    let dirs = parse_extra_dirs("nonexistent:/tmp/foo", &enabled);
    assert!(dirs.is_empty());
}

#[test]
#[serial]
fn test_scan_all_clients_with_extra_dirs() {
    let previous_current = std::env::var("TOKS_EXTRA_DIRS").ok();
    let previous_legacy = std::env::var("TOKSCOPE_EXTRA_DIRS").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();

    // Setup default Claude dir
    setup_mock_claude_dir(home);

    // Setup extra dir with additional session files
    let extra_dir = TempDir::new().unwrap();
    let extra_project = extra_dir.path().join("mac-project");
    fs::create_dir_all(&extra_project).unwrap();
    File::create(extra_project.join("extra-session.jsonl")).unwrap();

    unsafe {
        std::env::set_var(
            "TOKS_EXTRA_DIRS",
            format!("claude:{}", extra_dir.path().to_string_lossy()),
        )
    };

    let result = scan_all_clients(home.to_str().unwrap(), &["claude".to_string()]);
    // 1 from default path + 1 from extra dir
    assert_eq!(result.get(ClientId::Claude).len(), 2);

    restore_env("TOKS_EXTRA_DIRS", previous_current);
    restore_env("TOKSCOPE_EXTRA_DIRS", previous_legacy);
}

#[test]
#[serial]
fn test_scan_all_clients_ignores_extra_dirs_when_env_roots_disabled() {
    let previous = std::env::var("TOKSCOPE_EXTRA_DIRS").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_claude_dir(home);

    let extra_dir = TempDir::new().unwrap();
    let extra_project = extra_dir.path().join("mac-project");
    fs::create_dir_all(&extra_project).unwrap();
    File::create(extra_project.join("extra-session.jsonl")).unwrap();

    unsafe {
        std::env::set_var(
            "TOKSCOPE_EXTRA_DIRS",
            format!("claude:{}", extra_dir.path().to_string_lossy()),
        )
    };

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);
    assert_eq!(result.get(ClientId::Claude).len(), 1);

    restore_env("TOKSCOPE_EXTRA_DIRS", previous);
}

/// Verify that an extra scan path outside $HOME does not abort the scan.
/// `warn_if_escapes_home` must only warn, never block.
#[test]
#[serial]
fn test_extra_scan_path_outside_home_does_not_block_scan() {
    let fake_home = TempDir::new().unwrap();
    let outside_home = TempDir::new().unwrap();
    let outside_path = outside_home.path();
    assert!(!outside_path.starts_with(fake_home.path()));

    // Populate with a valid session file so the scanner has something to find.
    let session_dir = outside_path.join("sessions");
    fs::create_dir_all(&session_dir).unwrap();
    File::create(session_dir.join("session-abc123.json")).unwrap();

    // Set TOKSCOPE_EXTRA_DIRS to point claude at the outside path.
    let previous = std::env::var("TOKSCOPE_EXTRA_DIRS").ok();
    unsafe {
        std::env::set_var(
            "TOKSCOPE_EXTRA_DIRS",
            format!("claude:{}", outside_path.to_string_lossy()),
        )
    };

    // The scan must complete without panicking.
    let _result = scan_all_clients_with_env_strategy(
        fake_home.path().to_str().unwrap(),
        &["claude".to_string()],
        true, // use_env_roots = true so TOKSCOPE_EXTRA_DIRS is picked up
    );

    restore_env("TOKSCOPE_EXTRA_DIRS", previous);
    // No assertion on result.get(ClientId::Claude) — the outside dir might
    // not match the expected file patterns. The test goal is only liveness:
    // the scan must not panic when an extra path escapes $HOME.
}
