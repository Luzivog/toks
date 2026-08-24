use super::*;
use chrono::FixedOffset;

#[test]
fn warp_cache_parser_preserves_requests_and_spend_without_tokens() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "version": 1,
  "syncedAt": "2026-05-29T12:00:00Z",
  "usage": {
"requestsUsed": 42,
"requestLimit": 100,
"spendCents": 1234,
"nextRefreshTime": "2026-06-01T00:00:00Z"
  },
  "workspaces": [
{
  "id": "workspace-1",
  "name": "Personal",
  "requestsUsed": 12,
  "spendCents": 345
}
  ]
}"#,
    )
    .unwrap();

    let messages = crate::sessions::warp::parse_warp_file(file.path());
    assert_eq!(messages.len(), 1);

    let workspace = messages
        .iter()
        .find(|message| message.session_id == "warp-aggregate-workspace-1")
        .unwrap();
    assert_eq!(workspace.client, "warp");
    assert_eq!(workspace.model_id, "aggregate-requests");
    assert_eq!(workspace.provider_id, "warp");
    assert_eq!(workspace.workspace_label.as_deref(), Some("Personal"));
    assert_eq!(workspace.message_count, 12);
    assert_eq!(workspace.tokens, TokenBreakdown::default());
    assert!((workspace.cost - 3.45).abs() < 1e-9);

    std::fs::write(
        file.path(),
        r#"{
  "version": 1,
  "syncedAt": "2026-05-29T12:00:00Z",
  "usage": {
"requestsUsed": 42,
"requestLimit": 100,
"spendCents": 1234,
"nextRefreshTime": "2026-06-01T00:00:00Z"
  },
  "workspaces": []
}"#,
    )
    .unwrap();

    let messages = crate::sessions::warp::parse_warp_file(file.path());
    assert_eq!(messages.len(), 1);
    let account = &messages[0];
    assert_eq!(account.session_id, "warp-aggregate-account");
    assert_eq!(account.message_count, 42);
    assert_eq!(account.tokens, TokenBreakdown::default());
    assert!((account.cost - 12.34).abs() < 1e-9);
}

#[test]
fn test_timestamp_to_date_with_positive_offset() {
    let kst = FixedOffset::east_opt(9 * 60 * 60).unwrap();
    let ts = 1772512200000_i64; // 2026-03-03T04:30:00Z
    let date = timestamp_to_date_with_timezone(ts, &kst);
    assert_eq!(date, "2026-03-03");
}

#[test]
fn test_timestamp_to_date_with_negative_offset() {
    let pst = FixedOffset::west_opt(8 * 60 * 60).unwrap();
    let ts = 1772512200000_i64; // 2026-03-03T04:30:00Z
    let date = timestamp_to_date_with_timezone(ts, &pst);
    assert_eq!(date, "2026-03-02");
}

#[test]
fn test_timestamp_to_date_invalid_timestamp() {
    let utc = FixedOffset::east_opt(0).unwrap();
    let date = timestamp_to_date_with_timezone(i64::MAX, &utc);
    assert_eq!(date, "");
}

#[test]
fn test_unified_message_creation() {
    let tokens = TokenBreakdown {
        input: 100,
        output: 50,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };

    let msg = UnifiedMessage::new(
        "opencode",
        "claude-3-5-sonnet",
        "anthropic",
        "test-session-id",
        1733011200000,
        tokens,
        0.05,
    );

    assert_eq!(msg.client, "opencode");
    assert_eq!(msg.model_id, "claude-3-5-sonnet");
    assert_eq!(msg.session_id, "test-session-id");
    assert_eq!(msg.date, timestamp_to_date(1733011200000));
    assert_eq!(msg.cost, 0.05);
    assert_eq!(msg.agent, None);
    assert_eq!(msg.workspace_key, None);
    assert_eq!(msg.workspace_label, None);
}

#[test]
fn test_normalize_workspace_key_normalizes_slashes_and_trailing_separator() {
    assert_eq!(
        normalize_workspace_key(r"C:\Users\alice\repo\"),
        Some("C:/Users/alice/repo".to_string())
    );
    assert_eq!(
        normalize_workspace_key("/Users/alice//repo/"),
        Some("/Users/alice/repo".to_string())
    );
}

#[test]
fn test_normalize_workspace_key_preserves_unc_prefix() {
    assert_eq!(
        normalize_workspace_key(r"\\server\share\repo\"),
        Some("//server/share/repo".to_string())
    );
    assert_eq!(
        normalize_workspace_key("//server//share///repo/"),
        Some("//server/share/repo".to_string())
    );
}

#[test]
fn test_workspace_label_from_key_uses_last_path_segment() {
    assert_eq!(
        workspace_label_from_key("/Users/alice/my-repo"),
        Some("my-repo".to_string())
    );
    assert_eq!(
        workspace_label_from_key("encoded-project-key"),
        Some("encoded-project-key".to_string())
    );
}

#[test]
fn test_normalize_agent_name() {
    assert_eq!(normalize_agent_name("OmO"), "Sisyphus");
    assert_eq!(normalize_agent_name("Sisyphus"), "Sisyphus");
    assert_eq!(normalize_agent_name("omo"), "Sisyphus");
    assert_eq!(normalize_agent_name("sisyphus"), "Sisyphus");
    assert_eq!(
        normalize_agent_name("Sisyphus (Ultraworker)"),
        "Sisyphus (Ultraworker)"
    );

    assert_eq!(
        normalize_opencode_agent_name("Sisyphus (Ultraworker)"),
        "Sisyphus"
    );
    assert_eq!(normalize_opencode_agent_name("hephaestus"), "Hephaestus");
    assert_eq!(normalize_opencode_agent_name("prometheus"), "Prometheus");
    assert_eq!(normalize_opencode_agent_name("atlas"), "Atlas");
    assert_eq!(normalize_opencode_agent_name("metis"), "Metis");
    assert_eq!(normalize_opencode_agent_name("momus"), "Momus");
    assert_eq!(
        normalize_opencode_agent_name("sisyphus-junior"),
        "Sisyphus-Junior"
    );
    assert_eq!(
        normalize_opencode_agent_name("planner-sisyphus"),
        "Planner-Sisyphus"
    );

    assert_eq!(
        normalize_opencode_agent_name("Hephaestus (Deep Agent)"),
        "Hephaestus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Prometheus (Plan Builder)"),
        "Prometheus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Prometheus (Planner)"),
        "Prometheus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Atlas (Plan Executor)"),
        "Atlas"
    );
    assert_eq!(
        normalize_opencode_agent_name("Metis (Plan Consultant)"),
        "Metis"
    );
    assert_eq!(
        normalize_opencode_agent_name("Momus (Plan Critic)"),
        "Momus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Momus (Plan Reviewer)"),
        "Momus"
    );

    assert_eq!(normalize_agent_name("OmO-Plan"), "Planner-Sisyphus");
    assert_eq!(normalize_agent_name("Planner-Sisyphus"), "Planner-Sisyphus");
    assert_eq!(normalize_agent_name("omo-plan"), "Planner-Sisyphus");

    assert_eq!(normalize_agent_name("orchestrator-sisyphus"), "Atlas");
    assert_eq!(
        normalize_opencode_agent_name("orchestrator-sisyphus"),
        "Atlas"
    );
    assert_eq!(normalize_agent_name("explore"), "Explore");
    assert_eq!(normalize_agent_name("CustomAgent"), "CustomAgent");

    assert_eq!(normalize_agent_name("executor"), "Executor");
    assert_eq!(
        normalize_agent_name("task-orchestrator"),
        "Task Orchestrator"
    );
    assert_eq!(normalize_agent_name("git-committer"), "Git Committer");
    assert_eq!(
        normalize_agent_name("frontend-ui-ux-engineer"),
        "Frontend UI UX Engineer"
    );
    assert_eq!(
        normalize_agent_name("astrape:executor-high"),
        "Executor High"
    );
    assert_eq!(
        normalize_agent_name("oh-my-claudecode:code-reviewer"),
        "Code Reviewer"
    );
}

#[test]
fn test_normalize_copilot_agent_name() {
    assert_eq!(
        normalize_copilot_agent_name("github.copilot.default"),
        "GitHub Copilot"
    );
    assert_eq!(
        normalize_copilot_agent_name("GITHUB.COPILOT.DEFAULT"),
        "GitHub Copilot"
    );
    assert_eq!(normalize_copilot_agent_name("github.copilot.chat"), "Chat");
    assert_eq!(
        normalize_copilot_agent_name("Plugin:software-engineering-team:se-ux-ui-designer"),
        "Software Engineering Team: Se UX UI Designer"
    );
    assert_eq!(
        normalize_copilot_agent_name("plugin:my-team:my-agent"),
        "My Team: My Agent"
    );
    assert_eq!(
        normalize_copilot_agent_name("Plugin:code-review-team:api-reviewer"),
        "Code Review Team: API Reviewer"
    );
    assert_eq!(
        normalize_copilot_agent_name("some-custom-agent"),
        "Some Custom Agent"
    );
    assert_eq!(normalize_agent_name("oh-my-codex:librarian"), "Librarian");
    assert_eq!(normalize_agent_name("astrape:executor"), "Executor");
    assert_eq!(normalize_agent_name("plan-reviewer"), "Plan Reviewer");
    assert_eq!(normalize_agent_name("astrape:planner"), "Planner");

    assert_eq!(
        normalize_opencode_agent_name("astrape:sisyphus"),
        "Sisyphus"
    );
    assert_eq!(
        normalize_opencode_agent_name("oh-my-claudecode:executor"),
        "Executor"
    );

    // New dash format (oh-my-openagent current)
    assert_eq!(
        normalize_opencode_agent_name("Sisyphus - Ultraworker"),
        "Sisyphus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Hephaestus - Deep Agent"),
        "Hephaestus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Prometheus - Plan Builder"),
        "Prometheus"
    );
    assert_eq!(
        normalize_opencode_agent_name("Atlas - Plan Executor"),
        "Atlas"
    );
    assert_eq!(
        normalize_opencode_agent_name("Metis - Plan Consultant"),
        "Metis"
    );
    assert_eq!(
        normalize_opencode_agent_name("Momus - Plan Critic"),
        "Momus"
    );

    // ZWSP-prefixed names (oh-my-openagent sort-order prefixes)
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}Sisyphus - Ultraworker"),
        "Sisyphus"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}\u{200B}\u{200B}Prometheus - Plan Builder"),
        "Prometheus"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}\u{200B}\u{200B}\u{200B}Atlas - Plan Executor"),
        "Atlas"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{FEFF}Momus - Plan Critic"),
        "Momus"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}sisyphus-junior"),
        "Sisyphus-Junior"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}sisyphus"),
        "Sisyphus"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}  Sisyphus   -   Ultraworker  "),
        "Sisyphus"
    );
    assert_eq!(
        normalize_opencode_agent_name("\u{200B}\u{200B}\u{200B}   Prometheus    Plan Builder"),
        "Prometheus"
    );
}

#[test]
fn test_strip_zero_width_chars() {
    assert_eq!(strip_zero_width_chars("hello"), "hello");
    assert_eq!(strip_zero_width_chars("\u{200B}hello"), "hello");
    assert_eq!(
        strip_zero_width_chars("\u{200B}\u{200B}\u{200B}hello"),
        "hello"
    );
    assert_eq!(strip_zero_width_chars("\u{FEFF}hello"), "hello");
    assert_eq!(strip_zero_width_chars("\u{200C}hello\u{200D}"), "hello");
    assert_eq!(strip_zero_width_chars(""), "");
    assert_eq!(
        strip_zero_width_chars("no special chars"),
        "no special chars"
    );
}
