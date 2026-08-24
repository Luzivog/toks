use super::*;

#[test]
fn test_scan_all_clients_pi() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_pi_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["pi".to_string()], false);
    assert_eq!(result.get(ClientId::Pi).len(), 1);
    assert!(result.get(ClientId::OpenCode).is_empty());
    assert!(result.get(ClientId::Claude).is_empty());
}

#[test]
fn test_scan_all_clients_prime_agent_includes_root_and_rlm_child_sessions() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let root_dir = home.join(".prime/agent/sessions");
    let child_dir = home.join(".prime/agent/session-artifacts/root/sub-deadbeef");
    fs::create_dir_all(&root_dir).unwrap();
    fs::create_dir_all(&child_dir).unwrap();
    File::create(root_dir.join("root.jsonl")).unwrap();
    File::create(child_dir.join("child.jsonl")).unwrap();
    File::create(home.join(".prime/agent/session-artifacts/root/rlm-subagents.jsonl")).unwrap();

    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["prime-agent".to_string()],
        false,
    );

    assert_eq!(result.get(ClientId::PrimeAgent).len(), 2);
    assert!(result
        .get(ClientId::PrimeAgent)
        .iter()
        .any(|path| path.ends_with("root.jsonl")));
    assert!(result
        .get(ClientId::PrimeAgent)
        .iter()
        .any(|path| path.ends_with("child.jsonl")));
    assert!(result.get(ClientId::Pi).is_empty());
}

#[test]
#[serial]
fn test_scan_all_clients_prime_agent_honors_session_dir_override() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let custom_root = home.join("custom/sessions");
    let child_dir = home.join("custom/session-artifacts/root/sub-deadbeef");
    fs::create_dir_all(&custom_root).unwrap();
    fs::create_dir_all(&child_dir).unwrap();
    File::create(custom_root.join("root.jsonl")).unwrap();
    File::create(child_dir.join("child.jsonl")).unwrap();

    let mut env = EnvGuard::capture(&[
        "PRIME_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_DIR",
    ]);
    env.set("PRIME_AGENT_SESSION_DIR", "~/custom/sessions");
    env.remove("PRIME_AGENT_CODING_AGENT_SESSION_DIR");
    env.set(
        "PRIME_AGENT_CODING_AGENT_DIR",
        home.join("unused-agent-dir"),
    );
    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["prime-agent".to_string()],
        true,
    );

    assert_eq!(result.get(ClientId::PrimeAgent).len(), 2);
    assert!(result
        .get(ClientId::PrimeAgent)
        .iter()
        .all(|path| path.starts_with(home.join("custom"))));
}

#[test]
#[serial]
fn test_prime_agent_roots_honor_agent_dir_and_legacy_session_override() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let mut env = EnvGuard::capture(&[
        "PRIME_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_DIR",
    ]);
    env.remove("PRIME_AGENT_SESSION_DIR");
    env.remove("PRIME_AGENT_CODING_AGENT_SESSION_DIR");
    env.set("PRIME_AGENT_CODING_AGENT_DIR", "~/custom-agent");

    let roots = prime_agent_session_roots_with_env_strategy(home.to_str().unwrap(), true);
    assert_eq!(roots[0], home.join("custom-agent/sessions"));
    assert_eq!(roots[1], home.join("custom-agent/session-artifacts"));

    let legacy_sessions = home.join("legacy/sessions");
    env.set("PRIME_AGENT_CODING_AGENT_SESSION_DIR", &legacy_sessions);
    let roots = prime_agent_session_roots_with_env_strategy(home.to_str().unwrap(), true);
    assert_eq!(roots[0], legacy_sessions);
    assert_eq!(roots[1], home.join("legacy/session-artifacts"));

    // Match Prime Agent's `primary ?? legacy` environment lookup exactly:
    // an explicitly empty primary value suppresses the legacy variable,
    // then falls through to settings/default resolution.
    env.set("PRIME_AGENT_SESSION_DIR", "");
    let roots = prime_agent_session_roots_with_env_strategy(home.to_str().unwrap(), true);
    assert_eq!(roots[0], home.join("custom-agent/sessions"));
    assert_eq!(roots[1], home.join("custom-agent/session-artifacts"));
}

#[test]
fn test_prime_agent_tilde_expansion_matches_upstream_forward_slash_only() {
    let home = if cfg!(windows) {
        r"C:\Users\test"
    } else {
        "/tmp/home"
    };
    assert_eq!(
        expand_tilde_path_with_home("~/sessions", home),
        PathBuf::from(home).join("sessions")
    );
    assert_eq!(
        expand_tilde_path_with_home(r"~\sessions", home),
        PathBuf::from(r"~\sessions")
    );
}

#[test]
fn test_prime_agent_project_null_session_dir_resets_global_setting() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let global = home.join("global-settings.json");
    let project = home.join("project-settings.json");
    fs::write(&global, r#"{"sessionDir":"~/global-sessions"}"#).unwrap();
    fs::write(&project, r#"{"sessionDir":null}"#).unwrap();

    let setting = prime_agent_session_dir_from_settings_files(
        &global,
        Some(&project),
        home.to_str().unwrap(),
        Some(home),
    );
    assert_eq!(setting, Some(PrimeSessionDirSetting::Default));
    let sessions = match setting {
        Some(PrimeSessionDirSetting::Path(path))
        | Some(PrimeSessionDirSetting::CurrentDirectory(path)) => path,
        Some(PrimeSessionDirSetting::Default) | None => home.join("custom-agent/sessions"),
    };
    assert_eq!(sessions, home.join("custom-agent/sessions"));
}

#[test]
fn test_prime_agent_empty_project_session_dir_resolves_to_current_directory() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let current_dir = home.join("project");
    let global = home.join("global-settings.json");
    let project = home.join("project-settings.json");
    fs::write(&global, r#"{"sessionDir":"~/global-sessions"}"#).unwrap();
    fs::write(&project, r#"{"sessionDir":""}"#).unwrap();

    let setting = prime_agent_session_dir_from_settings_files(
        &global,
        Some(&project),
        home.to_str().unwrap(),
        Some(&current_dir),
    );
    assert_eq!(
        setting,
        Some(PrimeSessionDirSetting::CurrentDirectory(current_dir))
    );
}

#[test]
#[serial]
fn test_prime_agent_empty_session_dir_scans_cwd_root_and_artifacts() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let project = home.join("work/project");
    fs::create_dir_all(project.join(".prime/agent")).unwrap();
    fs::write(
        project.join(".prime/agent/settings.json"),
        r#"{"sessionDir":""}"#,
    )
    .unwrap();
    let root = project.join("root.jsonl");
    let child = project.join("session-artifacts/root/sub-child/child.jsonl");
    fs::create_dir_all(child.parent().unwrap()).unwrap();
    fs::write(
        &root, "{}
",
    )
    .unwrap();
    fs::write(
        &child, "{}
",
    )
    .unwrap();
    fs::write(
        project.join("session-artifacts/root/rlm-subagents.jsonl"),
        "{}
",
    )
    .unwrap();

    let mut env = EnvGuard::capture(&[
        "PRIME_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_DIR",
    ]);
    env.remove("PRIME_AGENT_SESSION_DIR");
    env.remove("PRIME_AGENT_CODING_AGENT_SESSION_DIR");
    env.remove("PRIME_AGENT_CODING_AGENT_DIR");
    let _current_dir = CurrentDirGuard::set(&project);
    let roots = prime_agent_session_roots_with_env_strategy(home.to_str().unwrap(), true);
    let result = scan_all_clients(home.to_str().unwrap(), &["prime-agent".to_string()]);

    let canonical_project = project.canonicalize().unwrap();
    assert_eq!(roots[0].canonicalize().unwrap(), canonical_project);
    assert_eq!(
        roots[1].canonicalize().unwrap(),
        project.join("session-artifacts").canonicalize().unwrap()
    );
    let files = result.get(ClientId::PrimeAgent);
    assert_eq!(files.len(), 2);
    let canonical_files: Vec<PathBuf> = files
        .iter()
        .map(|path| path.canonicalize().unwrap())
        .collect();
    assert!(canonical_files.contains(&root.canonicalize().unwrap()));
    assert!(canonical_files.contains(&child.canonicalize().unwrap()));
}

#[test]
#[serial]
fn test_prime_agent_roots_honor_settings_session_dir() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let agent_dir = home.join("custom-agent");
    fs::create_dir_all(&agent_dir).unwrap();
    fs::write(
        agent_dir.join("settings.json"),
        r#"{"sessionDir":"~/settings-sessions"}"#,
    )
    .unwrap();

    let mut env = EnvGuard::capture(&[
        "PRIME_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_SESSION_DIR",
        "PRIME_AGENT_CODING_AGENT_DIR",
    ]);
    env.remove("PRIME_AGENT_SESSION_DIR");
    env.remove("PRIME_AGENT_CODING_AGENT_SESSION_DIR");
    env.set("PRIME_AGENT_CODING_AGENT_DIR", &agent_dir);

    let roots = prime_agent_session_roots_with_env_strategy(home.to_str().unwrap(), true);
    assert_eq!(roots[0], home.join("settings-sessions"));
    assert_eq!(roots[1], home.join("session-artifacts"));
}

#[test]
fn test_scan_all_clients_kimchi() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_kimchi_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["kimchi".to_string()], false);
    assert_eq!(result.get(ClientId::Kimchi).len(), 1);
    assert!(result.get(ClientId::Kimchi)[0]
        .to_string_lossy()
        .ends_with(".jsonl"));
    assert!(result.get(ClientId::Pi).is_empty());
}

#[test]
fn test_scan_all_clients_senpi() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_senpi_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["senpi".to_string()], false);
    assert_eq!(result.get(ClientId::Senpi).len(), 1);
    assert!(result.get(ClientId::Senpi)[0]
        .ends_with("2026-07-29T15-19-53-436Z_019fae75-f35c-7b20-8d6f-e6dea8f7d9f5.jsonl"));
    assert!(result.get(ClientId::Pi).is_empty());
}

#[test]
fn test_scan_all_clients_senpi_is_not_scanned_as_pi() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_senpi_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["pi".to_string()], false);
    assert!(result.get(ClientId::Pi).is_empty());
    assert!(result.get(ClientId::Senpi).is_empty());
}

#[test]
fn test_scan_all_clients_omp_scanned_as_pi() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_omp_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["pi".to_string()], false);
    assert_eq!(result.get(ClientId::Pi).len(), 1);
    assert!(result.get(ClientId::Pi)[0].ends_with("2026-04-06T03-04-28Z_omp_ses_001.jsonl"));
    assert!(result.get(ClientId::OpenCode).is_empty());
}

#[test]
fn test_scan_all_clients_pi_from_both_paths() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_pi_dir(home);
    setup_mock_omp_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["pi".to_string()], false);
    assert_eq!(result.get(ClientId::Pi).len(), 2);
}
