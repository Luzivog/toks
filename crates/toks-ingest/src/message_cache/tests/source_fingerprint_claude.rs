use super::*;

#[test]
fn test_claude_sidechain_fingerprint_tracks_nested_parent_session_changes() {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("projects/project-one");
    let sidechain_path = project_dir
        .join("parent-session/subagents")
        .join("agent-child.jsonl");
    std::fs::create_dir_all(sidechain_path.parent().unwrap()).unwrap();
    std::fs::write(
        &sidechain_path,
        concat!(
            r#"{"type":"assistant","isSidechain":true,"sessionId":"parent-session","agentId":"child","timestamp":"2026-01-01T00:00:00Z","requestId":"req-1","message":{"id":"msg-1","model":"claude-sonnet-4","usage":{"input_tokens":1,"output_tokens":1}}}"#,
            "\n"
        ),
    )
    .unwrap();

    let parent_path = crate::sessions::claudecode::parent_session_paths_for_cache(&sidechain_path)
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(parent_path, project_dir.join("parent-session.jsonl"));
    let base = SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();

    std::fs::write(&parent_path, b"parent transcript 1\n").unwrap();
    let with_parent =
        SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();
    assert_ne!(base, with_parent);

    std::fs::write(&parent_path, b"parent transcript 2\n").unwrap();
    let updated_parent =
        SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();
    assert_ne!(with_parent, updated_parent);
}

#[test]
fn test_claude_sidechain_fingerprint_tracks_flat_parent_session_changes() {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("projects/project-one");
    std::fs::create_dir_all(&project_dir).unwrap();
    let sidechain_path = project_dir.join("agent-child.jsonl");
    let mut sidechain = format!("{}\n", "x".repeat(4096)).repeat(65);
    sidechain.push_str(concat!(
        r#"{"type":"assistant","isSidechain":true,"sessionId":"flat-parent","agentId":"child","timestamp":"2026-01-01T00:00:00Z","requestId":"req-1","message":{"id":"msg-1","model":"claude-sonnet-4","usage":{"input_tokens":1,"output_tokens":1}}}"#,
        "\n"
    ));
    std::fs::write(&sidechain_path, sidechain).unwrap();

    let parent_path = crate::sessions::claudecode::parent_session_paths_for_cache(&sidechain_path)
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(parent_path, project_dir.join("flat-parent.jsonl"));
    let base = SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();

    std::fs::write(&parent_path, b"flat parent 1\n").unwrap();
    let with_parent =
        SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();
    assert_ne!(base, with_parent);

    std::fs::write(&parent_path, b"flat parent 2\n").unwrap();
    let updated_parent =
        SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();
    assert_ne!(with_parent, updated_parent);
}

#[test]
fn test_claude_sidechain_warm_check_reuses_cached_parent_dependencies() {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("projects/project-one");
    std::fs::create_dir_all(&project_dir).unwrap();
    let sidechain_path = project_dir.join("agent-child.jsonl");
    let mut sidechain = format!("{}\n", "x".repeat(4096)).repeat(65);
    sidechain.push_str(concat!(
        r#"{"type":"assistant","isSidechain":true,"sessionId":"flat-parent","agentId":"child","timestamp":"2026-01-01T00:00:00Z","requestId":"req-1","message":{"id":"msg-1","model":"claude-sonnet-4","usage":{"input_tokens":1,"output_tokens":1}}}"#,
        "\n"
    ));
    std::fs::write(&sidechain_path, sidechain).unwrap();

    let cached = SourceFingerprint::from_claude_code_path_with_home(&sidechain_path, None).unwrap();
    let parent_path = project_dir.join("flat-parent.jsonl");
    assert!(cached.related_files.iter().any(|related| {
        related.suffix == "parent-session-0.jsonl"
            && related.path.to_path_buf() == parent_path
            && !related.exists
    }));
    assert!(matches!(
        SourceFingerprint::check_claude_code_path_with_home_samples_only(
            &sidechain_path,
            Some(&cached),
            None,
        ),
        Some(FingerprintStatus::Unchanged)
    ));

    std::fs::write(&parent_path, b"parent transcript\n").unwrap();
    assert!(matches!(
        SourceFingerprint::check_claude_code_path_with_home_samples_only(
            &sidechain_path,
            Some(&cached),
            None,
        ),
        Some(FingerprintStatus::Changed(_))
    ));
}

#[test]
fn test_claude_code_fingerprint_tracks_meta_sidecar_changes() {
    let dir = TempDir::new().unwrap();
    let jsonl_path = dir.path().join("agent-abc123.jsonl");
    std::fs::write(&jsonl_path, b"jsonl-content").unwrap();

    // No meta sidecar → baseline fingerprint
    let base = SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, None).unwrap();

    // Add meta sidecar → fingerprint changes
    let meta_path = dir.path().join("agent-abc123.meta.json");
    std::fs::write(&meta_path, br#"{"agentType":"explore"}"#).unwrap();
    let with_meta = SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, None).unwrap();
    assert_ne!(
        base, with_meta,
        "Adding meta sidecar should change fingerprint"
    );

    // Update meta sidecar → fingerprint changes again
    std::fs::write(&meta_path, br#"{"agentType":"executor"}"#).unwrap();
    let updated_meta =
        SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, None).unwrap();
    assert_ne!(
        with_meta, updated_meta,
        "Updating meta sidecar should change fingerprint"
    );

    // Main session file (no agent- prefix) → unaffected by unrelated meta files
    let main_path = dir.path().join("session-uuid.jsonl");
    std::fs::write(&main_path, b"main-session").unwrap();
    let main_fp1 = SourceFingerprint::from_claude_code_path_with_home(&main_path, None).unwrap();
    // Create a meta file with the main session stem (unlikely in practice)
    let main_meta = dir.path().join("session-uuid.meta.json");
    std::fs::write(&main_meta, br#"{"agentType":"x"}"#).unwrap();
    let main_fp2 = SourceFingerprint::from_claude_code_path_with_home(&main_path, None).unwrap();
    assert_ne!(
        main_fp1, main_fp2,
        "Claude Code fingerprints always track .meta.json if it exists"
    );
}

#[test]
fn test_claude_code_fingerprint_tracks_cc_mirror_variant_metadata_changes() {
    let dir = TempDir::new().unwrap();
    let variant_dir = dir.path().join(".cc-mirror/kimi-code");
    let config_dir = variant_dir.join("config");
    let project_dir = config_dir.join("projects/project-one");
    std::fs::create_dir_all(&project_dir).unwrap();
    let jsonl_path = project_dir.join("session.jsonl");
    std::fs::write(&jsonl_path, b"jsonl-content").unwrap();

    let variant_path = variant_dir.join("variant.json");
    std::fs::write(
        &variant_path,
        format!(
            r#"{{"name":"kimi-code","provider":"kimi","configDir":{}}}"#,
            json_path_literal(&config_dir)
        ),
    )
    .unwrap();
    let with_kimi = SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, None).unwrap();

    std::fs::write(
        &variant_path,
        format!(
            r#"{{"name":"kimi-code","provider":"minimax","configDir":{}}}"#,
            json_path_literal(&config_dir)
        ),
    )
    .unwrap();
    let with_minimax =
        SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, None).unwrap();

    assert_ne!(
        with_kimi, with_minimax,
        "Changing cc-mirror provider metadata should invalidate parsed Claude cache entries"
    );
}

#[test]
fn test_claude_code_fingerprint_tracks_cc_mirror_custom_config_dir_metadata_changes() {
    let dir = TempDir::new().unwrap();
    let variant_dir = dir.path().join(".cc-mirror/kimi-code");
    let config_dir = dir.path().join("mirror-configs/kimi-code");
    let project_dir = config_dir.join("projects/project-one");
    std::fs::create_dir_all(&project_dir).unwrap();
    let jsonl_path = project_dir.join("session.jsonl");
    std::fs::write(&jsonl_path, b"jsonl-content").unwrap();

    std::fs::create_dir_all(&variant_dir).unwrap();
    let variant_path = variant_dir.join("variant.json");
    std::fs::write(
        &variant_path,
        format!(
            r#"{{"name":"kimi-code","provider":"kimi","configDir":{}}}"#,
            json_path_literal(&config_dir)
        ),
    )
    .unwrap();
    let with_kimi =
        SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, Some(dir.path())).unwrap();

    std::fs::write(
        &variant_path,
        format!(
            r#"{{"name":"kimi-code","provider":"minimax","configDir":{}}}"#,
            json_path_literal(&config_dir)
        ),
    )
    .unwrap();
    let with_minimax =
        SourceFingerprint::from_claude_code_path_with_home(&jsonl_path, Some(dir.path())).unwrap();

    assert_ne!(
        with_kimi, with_minimax,
        "Changing cc-mirror metadata should invalidate cache entries for custom configDir layouts"
    );
}
