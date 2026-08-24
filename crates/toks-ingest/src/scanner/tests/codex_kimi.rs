use super::*;

#[test]
#[serial]
fn test_scan_all_clients_codex_with_env() {
    let previous_codex = std::env::var("CODEX_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codex_dir(home);

    // Set CODEX_HOME environment variable
    unsafe { std::env::set_var("CODEX_HOME", home.join(".codex")) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codex".to_string()]);
    assert_eq!(result.get(ClientId::Codex).len(), 1);

    restore_env("CODEX_HOME", previous_codex);
}

#[test]
#[serial]
fn test_scan_all_clients_codex_home_override_ignores_codex_home_env() {
    let previous_codex = std::env::var("CODEX_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("target-home");
    let conflicting = dir.path().join("conflicting-codex-home");
    setup_mock_codex_dir(&home);
    fs::create_dir_all(&conflicting).unwrap();

    unsafe { std::env::set_var("CODEX_HOME", &conflicting) };

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["codex".to_string()], false);
    assert_eq!(result.get(ClientId::Codex).len(), 1);
    assert!(result.get(ClientId::Codex)[0].ends_with("session.jsonl"));
    assert!(result.get(ClientId::Codex)[0].starts_with(home.join(".codex")));

    restore_env("CODEX_HOME", previous_codex);
}

#[test]
#[serial]
fn test_scan_all_clients_codex_archived_sessions() {
    let previous_codex = std::env::var("CODEX_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codex_archived_dir(home);

    unsafe { std::env::set_var("CODEX_HOME", home.join(".codex")) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codex".to_string()]);
    assert_eq!(result.get(ClientId::Codex).len(), 1);
    assert!(result.get(ClientId::Codex)[0].ends_with("archived.jsonl"));

    restore_env("CODEX_HOME", previous_codex);
}

#[test]
#[serial]
fn test_scan_all_clients_codex_sessions_and_archived() {
    let previous_codex = std::env::var("CODEX_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codex_dir(home);
    setup_mock_codex_archived_dir(home);

    unsafe { std::env::set_var("CODEX_HOME", home.join(".codex")) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codex".to_string()]);
    assert_eq!(result.get(ClientId::Codex).len(), 2);

    restore_env("CODEX_HOME", previous_codex);
}

#[test]
fn test_scan_all_clients_kimi() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_kimi_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["kimi".to_string()], false);
    assert_eq!(result.get(ClientId::Kimi).len(), 1);
    assert!(result.get(ClientId::Kimi)[0].ends_with("wire.jsonl"));
    assert!(result.get(ClientId::OpenCode).is_empty());
    assert!(result.get(ClientId::Claude).is_empty());
}

/// An explicit KIMI_CODE_HOME moves Kimi Code discovery to that root.
#[test]
#[serial]
fn test_scan_all_clients_kimi_code_home_override() {
    let mut env = EnvGuard::capture(&["KIMI_CODE_HOME"]);
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let custom_root = dir.path().join("custom-kimi-code");
    let wire = setup_mock_kimi_code_dir(&custom_root);
    env.set("KIMI_CODE_HOME", &custom_root);

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["kimi".to_string()]);

    assert_eq!(result.get(ClientId::Kimi), &vec![wire]);
}

/// Only *blank* values are reinterpreted. A non-blank value is used
/// verbatim, so a root whose name carries surrounding whitespace still
/// resolves — trimming the value here would silently miss it.
#[test]
#[serial]
// Unix-only because the fixture cannot exist on Windows: a directory name
// ending in a space is not addressable there. `CreateDirectoryW` strips the
// trailing space, so `<tmp>\ padded-kimi-code ` becomes
// `<tmp>\ padded-kimi-code`, and the very next call — which carries that
// component in the middle of a longer path, where no stripping happens —
// fails with ERROR_PATH_NOT_FOUND. The claim being made here (a padded
// KIMI_CODE_HOME is honored verbatim rather than trimmed) is also one
// Windows cannot violate: the OS trims the name before Toks sees a
// directory at all.
#[cfg(unix)]
fn test_scan_all_clients_kimi_code_home_override_is_not_trimmed() {
    let mut env = EnvGuard::capture(&["KIMI_CODE_HOME"]);
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let padded_root = dir.path().join(" padded-kimi-code ");
    let wire = setup_mock_kimi_code_dir(&padded_root);
    env.set("KIMI_CODE_HOME", &padded_root);

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["kimi".to_string()]);

    assert_eq!(result.get(ClientId::Kimi), &vec![wire]);
}

/// A blank KIMI_CODE_HOME — empty or whitespace-only, which is how
/// shells export an optional variable that was never given a value — means
/// "unset". Without this, `format!("{}/sessions", "")` walks the
/// root-level `/sessions` and the user's real sessions go unreported.
#[test]
#[serial]
fn test_scan_all_clients_kimi_code_home_blank_falls_back_to_home() {
    for blank in ["", "   ", "\t\n"] {
        let mut env = EnvGuard::capture(&["KIMI_CODE_HOME"]);
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let wire = setup_mock_kimi_code_dir(&home.join(".kimi-code"));
        env.set("KIMI_CODE_HOME", blank);

        let result = scan_without_extra_dirs(home.to_str().unwrap(), &["kimi".to_string()]);

        assert_eq!(
            result.get(ClientId::Kimi),
            &vec![wire],
            "KIMI_CODE_HOME={:?} must fall back to <home>/.kimi-code",
            blank
        );
    }
}

/// use_env_roots=false keeps `--home` authoritative: KIMI_CODE_HOME is
/// never consulted, blank or not.
#[test]
#[serial]
fn test_scan_all_clients_kimi_code_home_ignored_without_env_roots() {
    let mut env = EnvGuard::capture(&["KIMI_CODE_HOME"]);
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let conflicting = dir.path().join("conflicting-kimi-code");
    let wire = setup_mock_kimi_code_dir(&home.join(".kimi-code"));
    setup_mock_kimi_code_dir(&conflicting);
    env.set("KIMI_CODE_HOME", &conflicting);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["kimi".to_string()], false);

    assert_eq!(result.get(ClientId::Kimi), &vec![wire]);
}
