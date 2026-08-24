use super::*;

fn create_workflow_files(
    project: &str,
    parent_session: &str,
    workflow: &str,
    file_stem: &str,
    jsonl_content: &str,
    meta_content: Option<&str>,
) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let workflow_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join(project)
        .join(parent_session)
        .join("subagents")
        .join("workflows")
        .join(workflow);
    std::fs::create_dir_all(&workflow_dir).unwrap();

    let jsonl_path = workflow_dir.join(format!("{}.jsonl", file_stem));
    std::fs::write(&jsonl_path, jsonl_content).unwrap();

    if let Some(meta) = meta_content {
        let meta_path = workflow_dir.join(format!("{}.meta.json", file_stem));
        std::fs::write(&meta_path, meta).unwrap();
    }

    (temp_dir, jsonl_path)
}

#[test]
fn test_workflow_agent_transcript_counts_tokens() {
    // #815: agent-*.jsonl nested under subagents/workflows/<wf>/ is a real
    // transcript and its usage must be counted, keyed to the parent session.
    let jsonl = r#"{"type":"user","isSidechain":true,"sessionId":"wf-parent-001","agentId":"wfa1","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"do work"}}
{"type":"assistant","isSidechain":true,"sessionId":"wf-parent-001","agentId":"wfa1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_wf1","message":{"id":"msg_wf1","model":"claude-3-5-sonnet","usage":{"input_tokens":500,"output_tokens":200,"cache_read_input_tokens":100,"cache_creation_input_tokens":40}}}"#;
    let meta = r#"{"agentType":"workflow-subagent","spawnDepth":1}"#;

    let (_dir, path) = create_workflow_files(
        "myproject",
        "wf-parent-001",
        "wf_de048031",
        "agent-wfa1",
        jsonl,
        Some(meta),
    );
    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].session_id, "wf-parent-001",
        "deep nested transcript must key to the parent session, not the file/workflow"
    );
    assert_eq!(messages[0].tokens.input, 500);
    assert_eq!(messages[0].tokens.output, 200);
    assert_eq!(messages[0].tokens.cache_read, 100);
    assert_eq!(messages[0].tokens.cache_write, 40);
    assert_eq!(
        messages[0].agent,
        Some("Workflow Subagent".to_string()),
        "Tier 1 meta sidecar next to the deep nested transcript should resolve the name"
    );
}

#[test]
fn test_workflow_journal_not_ingested() {
    // #815 CRITICAL: journal.jsonl is workflow orchestration metadata, not a
    // transcript. Even if it grows lines that superficially resemble usage, the
    // parser must drop the whole file.
    let journal = r#"{"type":"started","key":"v2:abc","agentId":"wfa1"}
{"type":"result","key":"v2:abc","agentId":"wfa1","result":{"verdict":"needs_fixes","summary":"Input tokens are estimated"}}
{"type":"assistant","isSidechain":true,"sessionId":"wf-parent-002","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_j","message":{"id":"msg_j","model":"claude-3-5-sonnet","usage":{"input_tokens":9999,"output_tokens":9999}}}"#;

    let (_dir, path) = create_workflow_files(
        "myproject",
        "wf-parent-002",
        "wf_journal",
        "journal",
        journal,
        None,
    );
    assert_eq!(path.file_name().unwrap().to_str().unwrap(), "journal.jsonl");
    let messages = parse_claude_file(&path);

    assert!(
        messages.is_empty(),
        "journal.jsonl must never be ingested, even with usage-shaped lines; got {:?}",
        messages
    );
}

#[test]
fn test_tier2_deep_nested_workflow_recovers_agent() {
    // Deep nested layout without a meta sidecar: agent name must be recovered
    // from the parent session tool_use, proving find_parent_session_path walks
    // up past the extra workflows/<wf>/ levels.
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let parent_session_id = "deep-parent-uuid";
    let parent_content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"id":"msg_dp","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_deep","name":"Agent","input":{"subagent_type":"code-reviewer","prompt":"review"}}],"usage":{"input_tokens":80,"output_tokens":40}}}
{"type":"user","timestamp":"2024-12-01T10:00:01.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_deep","type":"tool_result","content":[{"type":"text","text":"agentId: deepagent1 (use SendMessage)"}]}]}}"#;
    std::fs::write(
        project_dir.join(format!("{}.jsonl", parent_session_id)),
        parent_content,
    )
    .unwrap();

    let workflow_dir = project_dir
        .join(parent_session_id)
        .join("subagents")
        .join("workflows")
        .join("wf_deep");
    std::fs::create_dir_all(&workflow_dir).unwrap();
    let sidechain_content = r#"{"type":"user","isSidechain":true,"sessionId":"deep-parent-uuid","agentId":"deepagent1","timestamp":"2024-12-01T10:00:00.500Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"deep-parent-uuid","agentId":"deepagent1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_deep","message":{"id":"msg_deep","model":"claude-3-5-sonnet","usage":{"input_tokens":300,"output_tokens":120}}}"#;
    let sidechain_path = workflow_dir.join("agent-deepagent1.jsonl");
    std::fs::write(&sidechain_path, sidechain_content).unwrap();

    let messages = parse_claude_file(&sidechain_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Code Reviewer".to_string()),
        "Tier 2 must resolve the agent name across the deep workflows/<wf>/ nesting"
    );
    assert_eq!(messages[0].session_id, parent_session_id);
    assert_eq!(messages[0].tokens.input, 300);
}
