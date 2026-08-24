use super::*;

#[test]
fn test_scan_all_clients_claude() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_claude_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);
    assert_eq!(result.get(ClientId::Claude).len(), 1);
    assert!(result.get(ClientId::OpenCode).is_empty());
}

/// Regression for #815: nested-layout subagent/workflow transcripts
/// (`<session>/subagents/workflows/<wf>/agent-*.jsonl`) must be discovered by
/// the recursive project-dir walk, so their usage is counted. The sibling
/// `journal.jsonl` orchestration metadata is discovered too, but the parser
/// drops it (covered in the claudecode parser tests).
#[test]
fn test_scan_all_clients_claude_nested_workflow_agents() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let wf = home.join(".claude/projects/myproject/sess-uuid/subagents/workflows/wf_abc");
    fs::create_dir_all(&wf).unwrap();
    let agent = wf.join("agent-a123.jsonl");
    File::create(&agent).unwrap().write_all(b"{}\n").unwrap();
    File::create(wf.join("journal.jsonl"))
        .unwrap()
        .write_all(b"{}\n")
        .unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);
    assert!(
        result.get(ClientId::Claude).iter().any(|p| p == &agent),
        "nested workflow agent transcript must be discovered, got {:?}",
        result.get(ClientId::Claude)
    );
}

#[test]
fn test_scan_all_clients_claude_transcripts() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_claude_dir(home);
    let transcript = setup_mock_claude_transcripts_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);

    assert_eq!(result.get(ClientId::Claude).len(), 2);
    assert!(
        result
            .get(ClientId::Claude)
            .iter()
            .any(|path| path == &transcript),
        "expected Claude transcript {} in {:?}",
        transcript.display(),
        result.get(ClientId::Claude)
    );
    assert!(result.get(ClientId::OpenCode).is_empty());
}

#[test]
fn test_scan_all_clients_claude_transcripts_without_projects_dir() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let transcript = setup_mock_claude_transcripts_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);

    assert_eq!(result.get(ClientId::Claude), &vec![transcript]);
    assert!(result.get(ClientId::OpenCode).is_empty());
}

#[test]
fn test_scan_all_clients_claude_discovers_cc_mirror_variant_projects() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_claude_dir(home);

    let variant_dir = home.join(".cc-mirror/kimi-code");
    let config_dir = variant_dir.join("config");
    let project_dir = config_dir.join("projects/project-one");
    fs::create_dir_all(&project_dir).unwrap();
    let variant_file = variant_dir.join("variant.json");
    fs::write(
        &variant_file,
        format!(
            r#"{{"name":"kimi-code","provider":"kimi","configDir":{}}}"#,
            json_path_literal(&config_dir)
        ),
    )
    .unwrap();
    let variant_session = project_dir.join("variant-session.jsonl");
    File::create(&variant_session).unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);

    assert_eq!(result.get(ClientId::Claude).len(), 2);
    assert!(
        result
            .get(ClientId::Claude)
            .iter()
            .any(|path| path == &variant_session),
        "expected cc-mirror session {} in {:?}",
        variant_session.display(),
        result.get(ClientId::Claude)
    );
}

#[test]
fn test_scan_all_clients_claude_dedups_cc_mirror_config_dir_pointing_at_normal_claude() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_claude_dir(home);

    let normal_claude_dir = home.join(".claude");
    let variant_dir = home.join(".cc-mirror/plain-mirror");
    fs::create_dir_all(&variant_dir).unwrap();
    fs::write(
        variant_dir.join("variant.json"),
        format!(
            r#"{{"name":"plain-mirror","provider":"mirror","configDir":{}}}"#,
            json_path_literal(&normal_claude_dir)
        ),
    )
    .unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["claude".to_string()], false);

    assert_eq!(
        result.get(ClientId::Claude).len(),
        1,
        "cc-mirror variants pointing at ~/.claude must not duplicate normal Claude files"
    );
}

#[test]
fn test_scan_all_clients_gemini() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_gemini_dir(home);

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["gemini".to_string()], false);
    assert_eq!(result.get(ClientId::Gemini).len(), 1);
    assert!(result.get(ClientId::OpenCode).is_empty());
}

#[test]
fn test_scan_all_clients_gemini_jsonl_session() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let gemini_path = home.join(".gemini/tmp/123/chats");
    fs::create_dir_all(&gemini_path).unwrap();
    File::create(gemini_path.join("session-abc.jsonl")).unwrap();

    let result =
        scan_all_clients_with_env_strategy(home.to_str().unwrap(), &["gemini".to_string()], false);
    assert_eq!(result.get(ClientId::Gemini).len(), 1);
    assert!(result.get(ClientId::Gemini)[0].ends_with("session-abc.jsonl"));
}

#[test]
fn test_scan_all_clients_openclaw_jsonl_only() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_openclaw_dir(home);

    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["openclaw".to_string()],
        false,
    );
    assert_eq!(result.get(ClientId::OpenClaw).len(), 3);
    assert!(result
        .get(ClientId::OpenClaw)
        .iter()
        .any(|path| path.ends_with("session-abc.jsonl")));
    assert!(result
        .get(ClientId::OpenClaw)
        .iter()
        .any(|path| path.ends_with("session-deleted.jsonl.deleted.123")));
    assert!(result
        .get(ClientId::OpenClaw)
        .iter()
        .any(|path| path.ends_with("session-reset.jsonl.reset.456")));
}

#[test]
fn test_scan_all_clients_openclaw_deleted_transcript() {
    let dir = TempDir::new().unwrap();
    let home = dir.path();

    let openclaw_sessions = home.join(".openclaw/agents/main/sessions");
    fs::create_dir_all(&openclaw_sessions).unwrap();
    File::create(openclaw_sessions.join("session-archived.jsonl.deleted.1700000000000")).unwrap();

    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["openclaw".to_string()],
        false,
    );
    assert_eq!(result.get(ClientId::OpenClaw).len(), 1);
    assert!(
        result.get(ClientId::OpenClaw)[0].ends_with("session-archived.jsonl.deleted.1700000000000")
    );
}
