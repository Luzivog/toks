use super::*;

// --- Sidechain / Agent tracking tests ---

/// Helper: create a sidechain JSONL file and optional meta sidecar in a nested layout.
fn create_sidechain_files(
    project: &str,
    parent_session: &str,
    agent_file_stem: &str,
    jsonl_content: &str,
    meta_content: Option<&str>,
) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let subagents_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join(project)
        .join(parent_session)
        .join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();

    let jsonl_path = subagents_dir.join(format!("{}.jsonl", agent_file_stem));
    std::fs::write(&jsonl_path, jsonl_content).unwrap();

    if let Some(meta) = meta_content {
        let meta_path = subagents_dir.join(format!("{}.meta.json", agent_file_stem));
        std::fs::write(&meta_path, meta).unwrap();
    }

    (temp_dir, jsonl_path)
}

#[test]
fn test_sidechain_nested_with_meta_sidecar() {
    let jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"parent-uuid-001","agentId":"abc123","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Find files"}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-uuid-001","agentId":"abc123","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_s01","message":{"id":"msg_s01","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":80,"cache_read_input_tokens":50}}}"#;
    let meta = r#"{"agentType":"explore","description":"Find session creation UI"}"#;

    let (_dir, path) = create_sidechain_files(
        "myproject",
        "parent-uuid-001",
        "agent-abc123",
        jsonl,
        Some(meta),
    );
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Explore".to_string()),
        "Should resolve agent name from meta sidecar and normalize"
    );
    assert_eq!(
        messages[0].session_id, "parent-uuid-001",
        "Should use parent session ID from transcript, not filename"
    );
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.output, 80);
    assert_eq!(messages[0].tokens.cache_read, 50);
}

#[test]
fn test_sidechain_nested_without_meta_falls_back() {
    let jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"parent-uuid-002","agentId":"def456","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Do something"}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-uuid-002","agentId":"def456","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_s02","message":{"id":"msg_s02","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":40}}}"#;

    let (_dir, path) =
        create_sidechain_files("myproject", "parent-uuid-002", "agent-def456", jsonl, None);
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Claude Code Subagent".to_string()),
        "Without meta sidecar, should fall back to generic label"
    );
    assert_eq!(messages[0].session_id, "parent-uuid-002");
}

/// Helper: create a deep nested-layout workflow transcript
/// `.../projects/<project>/<parent_session>/subagents/workflows/<wf>/<agent_stem>.jsonl`.

#[test]
fn test_sidechain_flat_legacy_layout() {
    // Flat layout: agent file lives directly under the project dir, no meta sidecar
    let jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"legacy-session-001","agentId":"ac0c74c","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Warmup"}}
{"type":"assistant","isSidechain":true,"sessionId":"legacy-session-001","agentId":"ac0c74c","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_l01","message":{"id":"msg_l01","model":"claude-3-5-sonnet","usage":{"input_tokens":150,"output_tokens":60}}}"#;

    let (_dir, path) = create_project_file(jsonl, "myproject", "agent-ac0c74c.jsonl");
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Claude Code Subagent".to_string()),
        "Legacy flat layout has no meta → Tier 3 fallback"
    );
    assert_eq!(
        messages[0].session_id, "legacy-session-001",
        "Should use parent session ID from transcript body"
    );
}

#[test]
fn test_sidechain_session_id_correction() {
    // Multiple sidechain files from the same parent should share the parent's session_id
    let make_jsonl = |agent_id: &str, req: &str, msg: &str| {
        format!(
            r#"{{"type":"user","isSidechain":true,"sessionId":"shared-parent-uuid","agentId":"{agent_id}","timestamp":"2024-12-01T10:00:00.000Z","message":{{"content":"task"}}}}
{{"type":"assistant","isSidechain":true,"sessionId":"shared-parent-uuid","agentId":"{agent_id}","timestamp":"2024-12-01T10:00:01.000Z","requestId":"{req}","message":{{"id":"{msg}","model":"claude-3-5-sonnet","usage":{{"input_tokens":100,"output_tokens":50}}}}}}"#
        )
    };

    let (_dir1, path1) = create_sidechain_files(
        "myproject",
        "shared-parent-uuid",
        "agent-aaa",
        &make_jsonl("aaa", "req_a", "msg_a"),
        Some(r#"{"agentType":"explore"}"#),
    );
    let (_dir2, path2) = create_sidechain_files(
        "myproject",
        "shared-parent-uuid",
        "agent-bbb",
        &make_jsonl("bbb", "req_b", "msg_b"),
        Some(r#"{"agentType":"executor"}"#),
    );
    let (_dir3, path3) = create_sidechain_files(
        "myproject",
        "shared-parent-uuid",
        "agent-ccc",
        &make_jsonl("ccc", "req_c", "msg_c"),
        None,
    );

    let msgs1 = parse_claude_file(&path1);
    let msgs2 = parse_claude_file(&path2);
    let msgs3 = parse_claude_file(&path3);

    // All three should share the parent session ID
    assert_eq!(msgs1[0].session_id, "shared-parent-uuid");
    assert_eq!(msgs2[0].session_id, "shared-parent-uuid");
    assert_eq!(msgs3[0].session_id, "shared-parent-uuid");

    // Agent names should differ
    assert_eq!(msgs1[0].agent, Some("Explore".to_string()));
    assert_eq!(msgs2[0].agent, Some("Executor".to_string()));
    assert_eq!(msgs3[0].agent, Some("Claude Code Subagent".to_string()));
}

#[test]
fn test_sidechain_token_totals_preserved() {
    // Verify that sidechain parsing doesn't change token accounting
    let sidechain_jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"parent-001","agentId":"xyz","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-001","agentId":"xyz","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_t1","message":{"id":"msg_t1","model":"claude-3-5-sonnet","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-001","agentId":"xyz","timestamp":"2024-12-01T10:00:02.000Z","requestId":"req_t2","message":{"id":"msg_t2","model":"claude-3-5-sonnet","usage":{"input_tokens":800,"output_tokens":300,"cache_read_input_tokens":150,"cache_creation_input_tokens":50}}}"#;

    let (_dir, path) = create_sidechain_files(
        "myproject",
        "parent-001",
        "agent-xyz",
        sidechain_jsonl,
        Some(r#"{"agentType":"code-reviewer"}"#),
    );
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 2);

    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|m| m.tokens.output).sum();
    let total_cache_read: i64 = messages.iter().map(|m| m.tokens.cache_read).sum();
    let total_cache_write: i64 = messages.iter().map(|m| m.tokens.cache_write).sum();

    assert_eq!(total_input, 1800, "input: 1000 + 800");
    assert_eq!(total_output, 800, "output: 500 + 300");
    assert_eq!(total_cache_read, 350, "cache_read: 200 + 150");
    assert_eq!(total_cache_write, 150, "cache_write: 100 + 50");

    // Both messages should have the same agent
    assert_eq!(messages[0].agent, Some("Code Reviewer".to_string()));
    assert_eq!(messages[1].agent, Some("Code Reviewer".to_string()));
}

#[test]
fn test_main_session_no_agent_regression() {
    // Non-sidechain (main session) files must produce agent: None
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Hello"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_m01","message":{"id":"msg_m01","model":"claude-3-5-sonnet","usage":{"input_tokens":500,"output_tokens":200}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:02.000Z","requestId":"req_m02","message":{"id":"msg_m02","model":"claude-3-5-sonnet","usage":{"input_tokens":600,"output_tokens":250}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].agent, None,
        "Main session messages must not have an agent"
    );
    assert_eq!(messages[1].agent, None);
}

#[test]
fn test_main_session_with_is_sidechain_false() {
    // Explicit isSidechain: false should be treated as main session
    let content = r#"{"type":"assistant","isSidechain":false,"timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent, None,
        "isSidechain=false should not set agent"
    );
}

#[test]
fn test_sidechain_dedup_preserves_agent() {
    // Streaming duplicates within a sidechain file should still carry the agent
    let jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"parent-dedup","agentId":"dd1","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-dedup","agentId":"dd1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_d1","message":{"id":"msg_d1","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":30}}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-dedup","agentId":"dd1","timestamp":"2024-12-01T10:00:01.100Z","requestId":"req_d1","message":{"id":"msg_d1","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":300}}}"#;

    let (_dir, path) = create_sidechain_files(
        "myproject",
        "parent-dedup",
        "agent-dd1",
        jsonl,
        Some(r#"{"agentType":"architect"}"#),
    );
    let messages = parse_claude_file(&path);

    assert_eq!(
        messages.len(),
        1,
        "Streaming duplicates should collapse to one"
    );
    assert_eq!(
        messages[0].tokens.output, 300,
        "Should keep max output_tokens"
    );
    assert_eq!(
        messages[0].agent,
        Some("Architect".to_string()),
        "Deduped message should retain agent"
    );
    assert_eq!(messages[0].session_id, "parent-dedup");
}

#[test]
fn test_sidechain_meta_with_omc_prefix_agent() {
    // Meta file might contain oh-my-claudecode: prefixed agent types
    let jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"parent-omc","agentId":"omc1","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-omc","agentId":"omc1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_omc","message":{"id":"msg_omc","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let (_dir, path) = create_sidechain_files(
        "myproject",
        "parent-omc",
        "agent-omc1",
        jsonl,
        Some(r#"{"agentType":"oh-my-claudecode:code-reviewer"}"#),
    );
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Code Reviewer".to_string()),
        "Should strip oh-my-claudecode: prefix and normalize"
    );
}

#[test]
fn test_sidechain_without_session_id_uses_filename() {
    // Edge case: sidechain entry without sessionId should fall back to filename stem
    let jsonl = r#"{"type":"user","isSidechain":true,"agentId":"noid","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"agentId":"noid","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_no","message":{"id":"msg_no","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let file = create_test_file(jsonl);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Claude Code Subagent".to_string()),
        "Still detected as sidechain"
    );
    // session_id should be the file stem (fallback)
    let expected_stem = file
        .path()
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(messages[0].session_id, expected_stem);
}
