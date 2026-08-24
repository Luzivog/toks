use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_parse_cline_valid_api_req_started() {
    let dir = TempDir::new().unwrap();
    let task_dir = dir.path().join("tasks").join("cline-task-1");
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(
            task_dir.join("ui_messages.json"),
            r#"[
  {
    "type": "say",
    "say": "api_req_started",
    "ts": "2026-02-18T12:00:00Z",
    "text": "{\"cost\":0.05,\"tokensIn\":40,\"tokensOut\":15,\"cacheReads\":7,\"cacheWrites\":3,\"apiProtocol\":\"anthropic\"}"
  }
]"#,
        )
        .unwrap();
    fs::write(
        task_dir.join("api_conversation_history.json"),
        r#"
<environment_details>
<model>claude-sonnet-4</model>
<name>ClineAgent</name>
</environment_details>
"#,
    )
    .unwrap();

    let messages = parse_cline_file(&task_dir.join("ui_messages.json"));
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "cline");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].model_id, "claude-sonnet-4");
    assert_eq!(messages[0].session_id, "cline-task-1");
    assert_eq!(messages[0].agent.as_deref(), Some("ClineAgent"));
    assert_eq!(messages[0].tokens.input, 40);
    assert_eq!(messages[0].tokens.output, 15);
    assert_eq!(messages[0].tokens.cache_read, 7);
    assert_eq!(messages[0].tokens.cache_write, 3);
    assert_eq!(messages[0].cost, 0.05);
}

#[test]
fn test_parse_cline_ignores_non_api_req_started_events() {
    let dir = TempDir::new().unwrap();
    let task_dir = dir.path().join("tasks").join("cline-task-2");
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(
            task_dir.join("ui_messages.json"),
            r#"[
  {
    "type": "say",
    "say": "assistant_message",
    "ts": "2026-02-18T12:00:00Z",
    "text": "{\"cost\":0.2,\"tokensIn\":10,\"tokensOut\":1,\"cacheReads\":0,\"cacheWrites\":0,\"apiProtocol\":\"anthropic\"}"
  }
]"#,
        )
        .unwrap();

    let messages = parse_cline_file(&task_dir.join("ui_messages.json"));
    assert!(messages.is_empty());
}

#[test]
fn test_parse_cline_cli_messages() {
    let dir = TempDir::new().unwrap();
    let session_dir = dir.path().join("sessions").join("cline-cli-session");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("cline-cli-session.json"),
        r#"{
  "session_id": "cline-cli-session",
  "provider": "cline-pass",
  "model": "cline-pass/glm-5.2",
  "workspace_root": "/home/example/project",
  "metadata": {"title": "CLI task"}
}"#,
    )
    .unwrap();
    fs::write(
        session_dir.join("cline-cli-session.messages.json"),
        r#"{
  "sessionId": "cline-cli-session",
  "agent": "lead",
  "messages": [
    {
      "role": "user",
      "ts": 1785320464923,
      "content": [{"type": "text", "text": "Inspect this project."}]
    },
    {
      "id": "msg-1",
      "role": "assistant",
      "ts": 1785320475705,
      "modelInfo": {"id": "cline-free/glm-5.2", "provider": "cline-pass"},
      "metrics": {
        "inputTokens": 7507,
        "outputTokens": 131,
        "cacheReadTokens": 50,
        "cacheWriteTokens": 0,
        "cost": 0.0110232
      }
    },
    {"role": "assistant", "metrics": {"inputTokens": 0, "outputTokens": 0}}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_cline_file(&session_dir.join("cline-cli-session.messages.json"));
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "cline");
    assert_eq!(messages[0].provider_id, "cline-pass");
    assert_eq!(messages[0].model_id, "cline-free/glm-5.2");
    assert_eq!(messages[0].session_id, "cline-cli-session");
    assert_eq!(messages[0].agent.as_deref(), Some("lead"));
    assert_eq!(messages[0].tokens.input, 7457);
    assert_eq!(messages[0].tokens.output, 131);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.cache_write, 0);
    assert_eq!(messages[0].cost, 0.0110232);
    assert_eq!(
        messages[0].cost_source,
        crate::sessions::CostSource::ProviderReported
    );
    assert_eq!(messages[0].workspace_label.as_deref(), Some("project"));
    assert_eq!(messages[0].session_title.as_deref(), Some("CLI task"));
    assert!(messages[0].is_turn_start);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("cline-cli:cline-cli-session:msg-1")
    );
}

#[test]
fn test_parse_cline_cli_turn_starts_ignore_tool_results() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("turns.messages.json");
    fs::write(
        &path,
        r#"{
  "sessionId": "turns",
  "messages": [
    {
      "id": "prompt-1",
      "role": "user",
      "content": [
        {"type": "text", "text": "Please inspect the repository."}
      ]
    },
    {
      "id": "assistant-tool-use",
      "role": "assistant",
      "content": [
        {"type": "text", "text": "I will inspect the repository."},
        {
          "type": "tool_use",
          "id": "tool-1",
          "name": "read_file",
          "input": {"path": "README.md"}
        },
        {"type": "future_block", "payload": {"priority": "low"}}
      ],
      "modelInfo": {"id": "provider/model", "provider": "provider"},
      "metrics": {
        "inputTokens": 100,
        "outputTokens": 20,
        "cacheReadTokens": 10,
        "cacheWriteTokens": 5,
        "cost": 0.02
      }
    },
    {
      "id": "tool-result",
      "role": "user",
      "content": [
        {
          "type": "tool_result",
          "tool_use_id": "tool-1",
          "content": [{"type": "text", "text": "README contents"}]
        }
      ]
    },
    {
      "id": "assistant-final",
      "role": "assistant",
      "content": [
        {"type": "text", "text": "The repository is ready."}
      ],
      "metrics": {
        "inputTokens": 15,
        "outputTokens": 6,
        "cacheReadTokens": 0,
        "cacheWriteTokens": 0
      }
    }
  ]
}"#,
    )
    .unwrap();

    let messages = parse_cline_cli_file(&path);

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("cline-cli:turns:assistant-tool-use")
    );
    assert_eq!(
        messages[1].dedup_key.as_deref(),
        Some("cline-cli:turns:assistant-final")
    );
    assert!(messages[0].is_turn_start);
    assert!(!messages[1].is_turn_start);
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.is_turn_start)
            .count(),
        1
    );
}

#[test]
fn test_parse_cline_cli_normalizes_cache_tokens_and_preserves_zero_cost() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("session.messages.json");
    fs::write(
        &path,
        r#"{
  "sessionId": "session-1",
  "messages": [
    {
      "id": "zero-cost",
      "role": "assistant",
      "metrics": {
        "inputTokens": 12,
        "outputTokens": 0,
        "cacheReadTokens": 5,
        "cacheWriteTokens": 2,
        "cost": 0
      }
    },
    {
      "id": "invalid-cost",
      "role": "assistant",
      "metrics": {
        "inputTokens": 1,
        "outputTokens": 2,
        "cost": "NaN"
      }
    }
  ]
}"#,
    )
    .unwrap();

    let messages = parse_cline_cli_file(&path);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 5);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.cache_write, 2);
    assert_eq!(messages[0].cost, 0.0);
    assert_eq!(
        messages[0].cost_source,
        crate::sessions::CostSource::ProviderReported
    );
    assert_eq!(messages[1].cost, 0.0);
    assert_eq!(
        messages[1].cost_source,
        crate::sessions::CostSource::Unknown
    );
}
