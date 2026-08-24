use super::*;

#[test]
#[serial]
fn test_env_guard_restores_after_unwind() {
    const KEY: &str = "TOKS_SCANNER_ENV_GUARD_SELF_CHECK";
    let mut outer = EnvGuard::capture(&[KEY]);
    outer.set(KEY, "before");
    let result = std::panic::catch_unwind(|| {
        let mut inner = EnvGuard::capture(&[KEY]);
        inner.set(KEY, "during");
        panic!("exercise EnvGuard unwinding");
    });
    assert!(result.is_err());
    assert_eq!(std::env::var_os(KEY), Some("before".into()));
}

#[test]
fn test_scan_result_total_files() {
    let mut result = ScanResult::default();
    result
        .get_mut(ClientId::OpenCode)
        .push(PathBuf::from("a.json"));
    result
        .get_mut(ClientId::OpenCode)
        .push(PathBuf::from("b.json"));
    result
        .get_mut(ClientId::Claude)
        .push(PathBuf::from("c.jsonl"));
    result
        .get_mut(ClientId::Gemini)
        .push(PathBuf::from("d.json"));
    result.get_mut(ClientId::Pi).push(PathBuf::from("e.jsonl"));
    assert_eq!(result.total_files(), 5);
}

#[test]
fn test_scan_result_all_files() {
    let mut result = ScanResult::default();
    result
        .get_mut(ClientId::OpenCode)
        .push(PathBuf::from("a.json"));
    result
        .get_mut(ClientId::Claude)
        .push(PathBuf::from("b.jsonl"));
    result
        .get_mut(ClientId::Codex)
        .push(PathBuf::from("c.jsonl"));
    result
        .get_mut(ClientId::Gemini)
        .push(PathBuf::from("d.json"));
    result
        .get_mut(ClientId::Cursor)
        .push(PathBuf::from("e.csv"));
    result.get_mut(ClientId::Pi).push(PathBuf::from("f.jsonl"));

    let all = result.all_files();
    assert_eq!(all.len(), 6);
    assert_eq!(all[0], (ClientId::OpenCode, PathBuf::from("a.json")));
    assert_eq!(all[1], (ClientId::Claude, PathBuf::from("b.jsonl")));
    assert_eq!(all[2], (ClientId::Codex, PathBuf::from("c.jsonl")));
    assert_eq!(all[3], (ClientId::Cursor, PathBuf::from("e.csv")));
    assert_eq!(all[4], (ClientId::Gemini, PathBuf::from("d.json")));
    assert_eq!(all[5], (ClientId::Pi, PathBuf::from("f.jsonl")));
}

#[test]
fn test_scan_result_empty() {
    let result = ScanResult::default();
    assert_eq!(result.total_files(), 0);
    assert!(result.all_files().is_empty());
}

#[test]
fn test_client_id_equality() {
    assert_eq!(ClientId::OpenCode, ClientId::OpenCode);
    assert_ne!(ClientId::OpenCode, ClientId::Claude);
}
