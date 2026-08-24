use super::*;

/// Write a gjc session JSONL file at
/// <home>/.gjc/agent/sessions/<slug>/<name> and return its path.
fn setup_mock_gjc_session(home: &Path, slug: &str, name: &str) -> PathBuf {
    let dir = home.join(".gjc/agent/sessions").join(slug);
    fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join(name);
    File::create(&file_path).unwrap();
    file_path
}

#[test]
#[serial]
fn test_gjc_discovery_recursive_glob_depth1_and_depth2() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    // depth 1: <slug>/<id>.jsonl
    setup_mock_gjc_session(home, "--work--proj--", "sess-001.jsonl");
    // depth 2: <slug>/<session>/N-Pass.jsonl
    let depth2 = home.join(".gjc/agent/sessions/--work--proj--/sess-001");
    fs::create_dir_all(&depth2).unwrap();
    File::create(depth2.join("0-Pass.jsonl")).unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["gjc".to_string()], false);
    assert_eq!(result.get(ClientId::Gjc).len(), 2);
}

#[test]
#[serial]
fn test_gjc_discovery_home_fallback_when_env_disabled() {
    let previous = std::env::var("GJC_CODING_AGENT_DIR").ok();
    // Even with the env var set, use_env_roots=false must ignore it and
    // read only the home fallback.
    let other = TempDir::new().unwrap();
    unsafe {
        std::env::set_var(
            "GJC_CODING_AGENT_DIR",
            other.path().to_string_lossy().as_ref(),
        )
    };

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_gjc_session(home, "slug", "a.jsonl");

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["gjc".to_string()], false);
    assert_eq!(result.get(ClientId::Gjc).len(), 1);

    restore_env("GJC_CODING_AGENT_DIR", previous);
}

#[test]
#[serial]
fn test_gjc_discovery_env_override() {
    let mut env = EnvGuard::capture(&[
        "GJC_CODING_AGENT_DIR",
        "GJC_CONFIG_DIR",
        "PI_CONFIG_DIR",
        "XDG_DATA_HOME",
        "TOKSCOPE_EXTRA_DIRS",
    ]);
    env.remove("GJC_CONFIG_DIR");
    env.remove("PI_CONFIG_DIR");
    env.remove("XDG_DATA_HOME");
    env.remove("TOKSCOPE_EXTRA_DIRS");

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    // Override target lives OUTSIDE ~/.gjc to prove the override is read.
    let agent_dir = dir.path().join("custom-gjc-agent");
    let override_sessions = agent_dir.join("sessions").join("slug");
    fs::create_dir_all(&override_sessions).unwrap();
    File::create(override_sessions.join("o.jsonl")).unwrap();

    env.set("GJC_CODING_AGENT_DIR", &agent_dir);

    let result = scan_all_clients(home.to_str().unwrap(), &["gjc".to_string()]);
    assert!(result
        .get(ClientId::Gjc)
        .iter()
        .any(|p| p.to_string_lossy().contains("custom-gjc-agent")));
}

#[test]
#[serial]
fn test_gjc_discovery_multi_root_files_dedup_to_one() {
    // When GJC_CODING_AGENT_DIR points at the same on-disk location the
    // home fallback also resolves, the file must be counted ONCE.
    let mut env = EnvGuard::capture(&[
        "GJC_CODING_AGENT_DIR",
        "GJC_CONFIG_DIR",
        "PI_CONFIG_DIR",
        "XDG_DATA_HOME",
        "TOKSCOPE_EXTRA_DIRS",
    ]);
    env.remove("GJC_CONFIG_DIR");
    env.remove("PI_CONFIG_DIR");
    env.remove("XDG_DATA_HOME");
    env.remove("TOKSCOPE_EXTRA_DIRS");

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_gjc_session(home, "slug", "dup.jsonl");

    // Point the env var at <home>/.gjc/agent so root (1) and root (4)
    // resolve to the same directory.
    let agent_dir = home.join(".gjc/agent");
    env.set("GJC_CODING_AGENT_DIR", &agent_dir);

    let result = scan_all_clients(home.to_str().unwrap(), &["gjc".to_string()]);
    assert_eq!(result.get(ClientId::Gjc).len(), 1);
}

// -----------------------------------------------------------------------
// Adversarial discovery tests for the gjc block
// -----------------------------------------------------------------------

/// (a) GJC_CONFIG_DIR set → <config>/agent/sessions/<slug>/x.jsonl discovered.
#[test]
#[serial]
fn test_gjc_discovery_gjc_config_dir() {
    let prev_agent = std::env::var("GJC_CODING_AGENT_DIR").ok();
    let prev_config = std::env::var("GJC_CONFIG_DIR").ok();
    let prev_pi = std::env::var("PI_CONFIG_DIR").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    // Clear all interfering env vars; we only want root (2) via GJC_CONFIG_DIR.
    unsafe {
        std::env::remove_var("GJC_CODING_AGENT_DIR");
        std::env::remove_var("PI_CONFIG_DIR");
        std::env::remove_var("XDG_DATA_HOME");
    }

    let home_dir = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();

    // Seed a file under the config-dir root.
    let sessions = config_dir.path().join("agent/sessions/my-slug");
    fs::create_dir_all(&sessions).unwrap();
    File::create(sessions.join("x.jsonl")).unwrap();

    unsafe {
        std::env::set_var(
            "GJC_CONFIG_DIR",
            config_dir.path().to_string_lossy().as_ref(),
        )
    };

    let result = scan_without_extra_dirs(home_dir.path().to_str().unwrap(), &["gjc".to_string()]);
    assert!(
        !result.get(ClientId::Gjc).is_empty(),
        "expected at least 1 file from GJC_CONFIG_DIR root, got {:?}",
        result.get(ClientId::Gjc)
    );
    assert!(
        result
            .get(ClientId::Gjc)
            .iter()
            .any(|p| p.to_string_lossy().contains("my-slug")),
        "discovered files should include the GJC_CONFIG_DIR session path"
    );

    restore_env("GJC_CODING_AGENT_DIR", prev_agent);
    restore_env("GJC_CONFIG_DIR", prev_config);
    restore_env("PI_CONFIG_DIR", prev_pi);
    restore_env("XDG_DATA_HOME", prev_xdg);
}

/// (b) PI_CONFIG_DIR set with GJC_CODING_AGENT_DIR and GJC_CONFIG_DIR unset →
///     <pi-config>/agent/sessions/<slug>/x.jsonl discovered.
#[test]
#[serial]
fn test_gjc_discovery_pi_config_dir() {
    let prev_agent = std::env::var("GJC_CODING_AGENT_DIR").ok();
    let prev_config = std::env::var("GJC_CONFIG_DIR").ok();
    let prev_pi = std::env::var("PI_CONFIG_DIR").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    unsafe {
        std::env::remove_var("GJC_CODING_AGENT_DIR");
        std::env::remove_var("GJC_CONFIG_DIR");
        std::env::remove_var("XDG_DATA_HOME");
    }

    let home_dir = TempDir::new().unwrap();
    let pi_config = TempDir::new().unwrap();

    let sessions = pi_config.path().join("agent/sessions/pi-slug");
    fs::create_dir_all(&sessions).unwrap();
    File::create(sessions.join("x.jsonl")).unwrap();

    unsafe { std::env::set_var("PI_CONFIG_DIR", pi_config.path().to_string_lossy().as_ref()) };

    let result = scan_without_extra_dirs(home_dir.path().to_str().unwrap(), &["gjc".to_string()]);
    assert!(
        !result.get(ClientId::Gjc).is_empty(),
        "expected at least 1 file from PI_CONFIG_DIR root, got {:?}",
        result.get(ClientId::Gjc)
    );
    assert!(
        result
            .get(ClientId::Gjc)
            .iter()
            .any(|p| p.to_string_lossy().contains("pi-slug")),
        "discovered files should include the PI_CONFIG_DIR session path"
    );

    restore_env("GJC_CODING_AGENT_DIR", prev_agent);
    restore_env("GJC_CONFIG_DIR", prev_config);
    restore_env("PI_CONFIG_DIR", prev_pi);
    restore_env("XDG_DATA_HOME", prev_xdg);
}

/// (c) XDG_DATA_HOME redirect — flattened path <xdg>/gjc/sessions/<slug>/x.jsonl
///     is discovered (the `agent/` segment is NOT present).
#[test]
#[serial]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn test_gjc_discovery_xdg_data_home_flattened() {
    let prev_agent = std::env::var("GJC_CODING_AGENT_DIR").ok();
    let prev_config = std::env::var("GJC_CONFIG_DIR").ok();
    let prev_pi = std::env::var("PI_CONFIG_DIR").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    unsafe {
        std::env::remove_var("GJC_CODING_AGENT_DIR");
        std::env::remove_var("GJC_CONFIG_DIR");
        std::env::remove_var("PI_CONFIG_DIR");
    }

    let home_dir = TempDir::new().unwrap();
    let xdg_data = TempDir::new().unwrap();

    // The XDG redirect flattens the `agent/` segment.
    let sessions = xdg_data.path().join("gjc/sessions/xdg-slug");
    fs::create_dir_all(&sessions).unwrap();
    File::create(sessions.join("x.jsonl")).unwrap();

    unsafe { std::env::set_var("XDG_DATA_HOME", xdg_data.path().to_string_lossy().as_ref()) };

    let result = scan_without_extra_dirs(home_dir.path().to_str().unwrap(), &["gjc".to_string()]);
    assert!(
        !result.get(ClientId::Gjc).is_empty(),
        "expected at least 1 file from XDG_DATA_HOME/gjc/sessions, got {:?}",
        result.get(ClientId::Gjc)
    );
    assert!(
        result
            .get(ClientId::Gjc)
            .iter()
            .any(|p| p.to_string_lossy().contains("xdg-slug")),
        "XDG redirect path must be discovered (flattened, no agent/ segment)"
    );

    restore_env("GJC_CODING_AGENT_DIR", prev_agent);
    restore_env("GJC_CONFIG_DIR", prev_config);
    restore_env("PI_CONFIG_DIR", prev_pi);
    restore_env("XDG_DATA_HOME", prev_xdg);
}

/// (d) Multi-root N4: home fallback file + XDG redirect file (DIFFERENT files,
///     different slugs) → count == 2.
#[test]
#[serial]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn test_gjc_discovery_multi_root_home_and_xdg_both_counted() {
    let prev_agent = std::env::var("GJC_CODING_AGENT_DIR").ok();
    let prev_config = std::env::var("GJC_CONFIG_DIR").ok();
    let prev_pi = std::env::var("PI_CONFIG_DIR").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    unsafe {
        std::env::remove_var("GJC_CODING_AGENT_DIR");
        std::env::remove_var("GJC_CONFIG_DIR");
        std::env::remove_var("PI_CONFIG_DIR");
    }

    let home_dir = TempDir::new().unwrap();
    let xdg_data = TempDir::new().unwrap();

    // Home fallback file.
    setup_mock_gjc_session(home_dir.path(), "home-slug", "home.jsonl");

    // XDG redirect file (different slug → distinct on-disk path, no dedup).
    let xdg_sessions = xdg_data.path().join("gjc/sessions/xdg-slug");
    fs::create_dir_all(&xdg_sessions).unwrap();
    File::create(xdg_sessions.join("xdg.jsonl")).unwrap();

    unsafe { std::env::set_var("XDG_DATA_HOME", xdg_data.path().to_string_lossy().as_ref()) };

    let result = scan_without_extra_dirs(home_dir.path().to_str().unwrap(), &["gjc".to_string()]);
    assert_eq!(
        result.get(ClientId::Gjc).len(),
        2,
        "both roots must contribute; files should NOT be collapsed to 1 (N4 push-all, not first-match). got {:?}",
        result.get(ClientId::Gjc)
    );

    restore_env("GJC_CODING_AGENT_DIR", prev_agent);
    restore_env("GJC_CONFIG_DIR", prev_config);
    restore_env("PI_CONFIG_DIR", prev_pi);
    restore_env("XDG_DATA_HOME", prev_xdg);
}

/// (e) use_env_roots=false ignores GJC_CONFIG_DIR and XDG_DATA_HOME even when
///     set, reading only the home fallback.
#[test]
#[serial]
fn test_gjc_discovery_use_env_roots_false_ignores_config_and_xdg() {
    let prev_agent = std::env::var("GJC_CODING_AGENT_DIR").ok();
    let prev_config = std::env::var("GJC_CONFIG_DIR").ok();
    let prev_pi = std::env::var("PI_CONFIG_DIR").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let home_dir = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let xdg_data = TempDir::new().unwrap();

    // Seed a home-fallback file.
    setup_mock_gjc_session(home_dir.path(), "home-slug", "home.jsonl");

    // Seed a GJC_CONFIG_DIR file — must be ignored.
    let config_sessions = config_dir.path().join("agent/sessions/cfg-slug");
    fs::create_dir_all(&config_sessions).unwrap();
    File::create(config_sessions.join("cfg.jsonl")).unwrap();

    // Seed an XDG file — must be ignored.
    let xdg_sessions = xdg_data.path().join("gjc/sessions/xdg-slug");
    fs::create_dir_all(&xdg_sessions).unwrap();
    File::create(xdg_sessions.join("xdg.jsonl")).unwrap();

    unsafe {
        std::env::remove_var("GJC_CODING_AGENT_DIR");
        std::env::set_var(
            "GJC_CONFIG_DIR",
            config_dir.path().to_string_lossy().as_ref(),
        );
        std::env::set_var("XDG_DATA_HOME", xdg_data.path().to_string_lossy().as_ref());
    }

    let result = scan_all_clients_with_env_strategy(
        home_dir.path().to_str().unwrap(),
        &["gjc".to_string()],
        false, // use_env_roots = false
    );

    assert_eq!(
        result.get(ClientId::Gjc).len(),
        1,
        "use_env_roots=false must suppress GJC_CONFIG_DIR and XDG_DATA_HOME, yielding only the home fallback. got {:?}",
        result.get(ClientId::Gjc)
    );
    assert!(
        result
            .get(ClientId::Gjc)
            .iter()
            .any(|p| p.to_string_lossy().contains("home-slug")),
        "the sole discovered file must be from the home fallback"
    );

    restore_env("GJC_CODING_AGENT_DIR", prev_agent);
    restore_env("GJC_CONFIG_DIR", prev_config);
    restore_env("PI_CONFIG_DIR", prev_pi);
    restore_env("XDG_DATA_HOME", prev_xdg);
}

/// (f) Nonexistent GJC_CODING_AGENT_DIR does not panic and yields only the
///     home fallback file.
#[test]
#[serial]
fn test_gjc_discovery_nonexistent_agent_dir_no_panic() {
    let prev_agent = std::env::var("GJC_CODING_AGENT_DIR").ok();
    let prev_config = std::env::var("GJC_CONFIG_DIR").ok();
    let prev_pi = std::env::var("PI_CONFIG_DIR").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let home_dir = TempDir::new().unwrap();

    // Point GJC_CODING_AGENT_DIR at a path that does not exist.
    unsafe {
        std::env::set_var(
            "GJC_CODING_AGENT_DIR",
            "/nonexistent/path/that/does/not/exist",
        );
        std::env::remove_var("GJC_CONFIG_DIR");
        std::env::remove_var("PI_CONFIG_DIR");
        std::env::remove_var("XDG_DATA_HOME");
    }

    // Seed a home-fallback file so there is something to discover.
    setup_mock_gjc_session(home_dir.path(), "slug", "a.jsonl");

    // Must not panic.
    let result = scan_without_extra_dirs(home_dir.path().to_str().unwrap(), &["gjc".to_string()]);

    assert_eq!(
        result.get(ClientId::Gjc).len(),
        1,
        "nonexistent GJC_CODING_AGENT_DIR should be silently skipped, home fallback must still be found. got {:?}",
        result.get(ClientId::Gjc)
    );

    restore_env("GJC_CODING_AGENT_DIR", prev_agent);
    restore_env("GJC_CONFIG_DIR", prev_config);
    restore_env("PI_CONFIG_DIR", prev_pi);
    restore_env("XDG_DATA_HOME", prev_xdg);
}
