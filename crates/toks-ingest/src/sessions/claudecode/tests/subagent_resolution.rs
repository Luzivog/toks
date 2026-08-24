use super::*;

#[test]
fn parent_probe_resolves_first_row_marker_under_tiny_budget() {
    let session_id = "11111111-2222-3333-4444-555555555555";
    let content =
        format!("{{\"isSidechain\":true,\"sessionId\":\"{session_id}\",\"type\":\"user\"}}\n");
    let (_dir, path) = create_project_file(&content, "proj", "agent-abc.jsonl");

    // The current row is always parsed in full, so a first-row marker is
    // found even with a probe budget smaller than the row itself.
    let candidates = parent_session_paths_for_cache_bounded(&path, 8);
    assert!(candidates.iter().any(|candidate| {
        candidate.file_name().and_then(|name| name.to_str())
            == Some("11111111-2222-3333-4444-555555555555.jsonl")
    }));
}

#[test]
fn parent_probe_stops_at_byte_cap_for_late_marker() {
    let session_id = "11111111-2222-3333-4444-555555555555";
    let mut content = String::new();
    while content.len() < 4096 {
        content.push_str("{\"type\":\"summary\",\"isSidechain\":false}\n");
    }
    content.push_str(&format!(
        "{{\"isSidechain\":true,\"sessionId\":\"{session_id}\",\"type\":\"user\"}}\n"
    ));
    let (_dir, path) = create_project_file(&content, "proj", "agent-late.jsonl");

    // A budget smaller than the marker's offset gives up before reaching it.
    assert!(parent_session_paths_for_cache_bounded(&path, 1024).is_empty());
    // An ample budget still finds it — the cap only bounds marker-less scans.
    assert!(!parent_session_paths_for_cache_bounded(&path, 1024 * 1024).is_empty());
}

// --- Tier 2: parent session tool_use inference tests ---

#[test]
fn test_tier2_recovers_agent_from_parent_tool_use() {
    // Nested layout: sidechain without meta, but parent session has matching tool_use
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create parent session file with tool_use (Agent) and tool_result (agentId)
    let parent_session_id = "parent-tier2-uuid";
    let parent_content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"id":"msg_p1","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_abc","name":"Agent","input":{"subagent_type":"document-specialist","prompt":"Research something"}}],"usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"user","timestamp":"2024-12-01T10:00:01.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_abc","type":"tool_result","content":[{"type":"text","text":"Found the docs"},{"type":"text","text":"agentId: t2agent1 (use SendMessage with to: 't2agent1' to continue this agent)\n<usage>total_tokens: 5000</usage>"}]}]}}"#;
    let parent_path = project_dir.join(format!("{}.jsonl", parent_session_id));
    std::fs::write(&parent_path, parent_content).unwrap();

    // Create sidechain file (nested layout, no meta sidecar)
    let subagents_dir = project_dir.join(parent_session_id).join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();
    let sidechain_content = r#"{"type":"user","isSidechain":true,"sessionId":"parent-tier2-uuid","agentId":"t2agent1","timestamp":"2024-12-01T10:00:00.500Z","message":{"content":"Research something"}}
{"type":"assistant","isSidechain":true,"sessionId":"parent-tier2-uuid","agentId":"t2agent1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_t2","message":{"id":"msg_t2","model":"claude-3-5-sonnet","usage":{"input_tokens":300,"output_tokens":120}}}"#;
    let sidechain_path = subagents_dir.join("agent-t2agent1.jsonl");
    std::fs::write(&sidechain_path, sidechain_content).unwrap();

    let messages = parse_claude_file(&sidechain_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Document Specialist".to_string()),
        "Tier 2 should recover agent name from parent tool_use"
    );
    assert_eq!(messages[0].session_id, parent_session_id);
}

#[test]
fn test_tier2_flat_layout_recovers_agent() {
    // Flat layout: sidechain file in same dir as parent, no meta sidecar
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let parent_session_id = "flat-parent-uuid";
    let parent_content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"id":"msg_fp","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_flat","name":"Agent","input":{"subagent_type":"explore","prompt":"Find files"}}],"usage":{"input_tokens":50,"output_tokens":30}}}
{"type":"user","timestamp":"2024-12-01T10:00:01.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_flat","type":"tool_result","content":[{"type":"text","text":"agentId: flatagent1 (use SendMessage)"}]}]}}"#;
    std::fs::write(
        project_dir.join(format!("{}.jsonl", parent_session_id)),
        parent_content,
    )
    .unwrap();

    let sidechain_content = r#"{"type":"user","isSidechain":true,"sessionId":"flat-parent-uuid","agentId":"flatagent1","timestamp":"2024-12-01T10:00:00.500Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"flat-parent-uuid","agentId":"flatagent1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_flat","message":{"id":"msg_flat","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    std::fs::write(
        project_dir.join("agent-flatagent1.jsonl"),
        sidechain_content,
    )
    .unwrap();

    let messages = parse_claude_file(&project_dir.join("agent-flatagent1.jsonl"));

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Explore".to_string()),
        "Tier 2 should work for flat layout too"
    );
}

#[test]
fn test_tier1_takes_precedence_over_tier2() {
    // When meta sidecar exists, Tier 1 wins even if parent has a different subagent_type
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let parent_session_id = "precedence-parent";
    let parent_content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"id":"msg_prec","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_prec","name":"Agent","input":{"subagent_type":"wrong-type","prompt":"task"}}],"usage":{"input_tokens":50,"output_tokens":30}}}
{"type":"user","timestamp":"2024-12-01T10:00:01.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_prec","type":"tool_result","content":[{"type":"text","text":"agentId: precagent1 done"}]}]}}"#;
    std::fs::write(
        project_dir.join(format!("{}.jsonl", parent_session_id)),
        parent_content,
    )
    .unwrap();

    let subagents_dir = project_dir.join(parent_session_id).join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();

    let sidechain_content = r#"{"type":"user","isSidechain":true,"sessionId":"precedence-parent","agentId":"precagent1","timestamp":"2024-12-01T10:00:00.500Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"precedence-parent","agentId":"precagent1","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_prec","message":{"id":"msg_prec2","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    std::fs::write(
        subagents_dir.join("agent-precagent1.jsonl"),
        sidechain_content,
    )
    .unwrap();
    std::fs::write(
        subagents_dir.join("agent-precagent1.meta.json"),
        r#"{"agentType":"code-reviewer"}"#,
    )
    .unwrap();

    let messages = parse_claude_file(&subagents_dir.join("agent-precagent1.jsonl"));

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent,
        Some("Code Reviewer".to_string()),
        "Tier 1 (meta sidecar) should take precedence over Tier 2 (parent lookup)"
    );
}

#[test]
fn test_extract_agent_id_from_text() {
    assert_eq!(
        extract_agent_id_from_text(
            "agentId: a8f80f8f33163def2 (use SendMessage with to: 'a8f80f8f33163def2')"
        ),
        Some("a8f80f8f33163def2".to_string())
    );
    assert_eq!(
        extract_agent_id_from_text("agentId: abc123\n<usage>total_tokens: 5000</usage>"),
        Some("abc123".to_string())
    );
    assert_eq!(extract_agent_id_from_text("no agent id here"), None);
    assert_eq!(
        extract_agent_id_from_text("agentId: "),
        None,
        "Empty agent id should return None"
    );
}

#[test]
fn test_sidechain_agent_id_from_stem_extracts_aside_question_suffix() {
    assert_eq!(
        sidechain_agent_id_from_stem("agent-aside_question-0320a3d71bc1d01e"),
        Some("0320a3d71bc1d01e".to_string())
    );
    assert_eq!(
        sidechain_agent_id_from_stem("agent-flatagent1"),
        Some("flatagent1".to_string())
    );
}

#[test]
fn test_tier2_uses_entry_agent_id_when_filename_prefix_differs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let parent_session_id = "aside-parent-uuid";
    let parent_content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"id":"msg_aside_parent","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_aside","name":"Agent","input":{"subagent_type":"writer","prompt":"Summarize findings"}}],"usage":{"input_tokens":50,"output_tokens":30}}}
{"type":"user","timestamp":"2024-12-01T10:00:01.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_aside","type":"tool_result","content":[{"type":"text","text":"agentId: 0320a3d71bc1d01e (use SendMessage)"}]}]}}"#;
    std::fs::write(
        project_dir.join(format!("{}.jsonl", parent_session_id)),
        parent_content,
    )
    .unwrap();

    let subagents_dir = project_dir.join(parent_session_id).join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();
    let sidechain_content = r#"{"type":"user","isSidechain":true,"sessionId":"aside-parent-uuid","agentId":"0320a3d71bc1d01e","timestamp":"2024-12-01T10:00:00.500Z","message":{"content":"task"}}
{"type":"assistant","isSidechain":true,"sessionId":"aside-parent-uuid","agentId":"0320a3d71bc1d01e","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_aside","message":{"id":"msg_aside","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let sidechain_path = subagents_dir.join("agent-aside_question-0320a3d71bc1d01e.jsonl");
    std::fs::write(&sidechain_path, sidechain_content).unwrap();

    let messages = parse_claude_file(&sidechain_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent, Some("Writer".to_string()));
}

#[test]
fn test_parent_subagent_lookup_cache_reuses_parsed_parent_results() {
    let temp_dir = tempfile::tempdir().unwrap();
    let parent_path = temp_dir.path().join("parent.jsonl");
    let initial_parent = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_a","name":"Agent","input":{"subagent_type":"explore"}},{"type":"tool_use","id":"toolu_b","name":"Agent","input":{"subagent_type":"executor"}}]}}
{"type":"user","message":{"content":[{"tool_use_id":"toolu_a","type":"tool_result","content":[{"type":"text","text":"agentId: cacheA"}]},{"tool_use_id":"toolu_b","type":"tool_result","content":[{"type":"text","text":"agentId: cacheB"}]}]}}"#;
    std::fs::write(&parent_path, initial_parent).unwrap();

    let mut parent_cache = ParentSubagentTypeCache::new();
    assert_eq!(
        lookup_subagent_type_in_parent(&parent_path, "cacheA", &mut parent_cache),
        Some("explore".to_string())
    );

    std::fs::write(
            &parent_path,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_b","name":"Agent","input":{"subagent_type":"writer"}}]}}"#,
        )
        .unwrap();

    assert_eq!(
        lookup_subagent_type_in_parent(&parent_path, "cacheB", &mut parent_cache),
        Some("executor".to_string())
    );
}

#[test]
fn test_tier2_multiple_agents_in_same_parent() {
    // Parent spawns multiple agents; each sidechain should get the correct type
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let parent_session_id = "multi-agent-parent";
    let parent_content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"id":"msg_ma1","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_m1","name":"Agent","input":{"subagent_type":"explore","prompt":"find files"}}],"usage":{"input_tokens":50,"output_tokens":30}}}
{"type":"user","timestamp":"2024-12-01T10:00:01.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_m1","type":"tool_result","content":[{"type":"text","text":"agentId: multiA1 done"}]}]}}
{"type":"assistant","timestamp":"2024-12-01T10:00:02.000Z","message":{"id":"msg_ma2","model":"claude-3-5-sonnet","role":"assistant","content":[{"type":"tool_use","id":"toolu_m2","name":"Agent","input":{"subagent_type":"executor","prompt":"implement feature"}}],"usage":{"input_tokens":60,"output_tokens":40}}}
{"type":"user","timestamp":"2024-12-01T10:00:03.000Z","message":{"role":"user","content":[{"tool_use_id":"toolu_m2","type":"tool_result","content":[{"type":"text","text":"agentId: multiB2 done"}]}]}}"#;
    std::fs::write(
        project_dir.join(format!("{}.jsonl", parent_session_id)),
        parent_content,
    )
    .unwrap();

    let subagents_dir = project_dir.join(parent_session_id).join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();

    let make_sidechain = |agent_id: &str| {
        format!(
            r#"{{"type":"user","isSidechain":true,"sessionId":"{parent_session_id}","agentId":"{agent_id}","timestamp":"2024-12-01T10:00:00.500Z","message":{{"content":"task"}}}}
{{"type":"assistant","isSidechain":true,"sessionId":"{parent_session_id}","agentId":"{agent_id}","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_{agent_id}","message":{{"id":"msg_{agent_id}","model":"claude-3-5-sonnet","usage":{{"input_tokens":100,"output_tokens":50}}}}}}"#
        )
    };

    std::fs::write(
        subagents_dir.join("agent-multiA1.jsonl"),
        make_sidechain("multiA1"),
    )
    .unwrap();
    std::fs::write(
        subagents_dir.join("agent-multiB2.jsonl"),
        make_sidechain("multiB2"),
    )
    .unwrap();

    let msgs_a = parse_claude_file(&subagents_dir.join("agent-multiA1.jsonl"));
    let msgs_b = parse_claude_file(&subagents_dir.join("agent-multiB2.jsonl"));

    assert_eq!(
        msgs_a[0].agent,
        Some("Explore".to_string()),
        "First agent should be explore"
    );
    assert_eq!(
        msgs_b[0].agent,
        Some("Executor".to_string()),
        "Second agent should be executor"
    );
}
