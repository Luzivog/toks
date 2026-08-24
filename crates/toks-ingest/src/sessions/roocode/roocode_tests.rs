use super::*;
use std::fs;
use tempfile::TempDir;

fn setup_task(
    dir: &TempDir,
    task_id: &str,
    ui_messages_content: &str,
    history_content: Option<&str>,
) -> PathBuf {
    let task_dir = dir.path().join("tasks").join(task_id);
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(task_dir.join("ui_messages.json"), ui_messages_content).unwrap();
    if let Some(history) = history_content {
        fs::write(task_dir.join("api_conversation_history.json"), history).unwrap();
    }
    task_dir.join("ui_messages.json")
}

#[test]
fn test_parse_roocode_valid_api_req_started() {
    let dir = TempDir::new().unwrap();
    let ui_messages = r#"[
  {
"type": "say",
"say": "api_req_started",
"ts": "2026-02-18T12:00:00Z",
"text": "{\"cost\":0.12,\"tokensIn\":100,\"tokensOut\":50,\"cacheReads\":20,\"cacheWrites\":5,\"apiProtocol\":\"anthropic\"}"
  },
  {
"type": "say",
"say": "assistant_message",
"ts": "2026-02-18T12:00:01Z",
"text": "{}"
  }
]"#;
    let history = r#"before
<environment_details>
<model>claude-sonnet-4</model>
<slug>architect</slug>
<name>Architect</name>
</environment_details>
after"#;
    let path = setup_task(&dir, "task-abc", ui_messages, Some(history));

    let messages = parse_roocode_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "roocode");
    assert_eq!(messages[0].model_id, "claude-sonnet-4");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].session_id, "task-abc");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.cache_write, 5);
    assert_eq!(messages[0].cost, 0.12);
    assert_eq!(messages[0].agent.as_deref(), Some("architect"));
}

#[test]
fn test_parse_roocode_skips_malformed_payload_entry() {
    let dir = TempDir::new().unwrap();
    let ui_messages = r#"[
  {
"type": "say",
"say": "api_req_started",
"ts": "2026-02-18T12:00:00Z",
"text": "not-json"
  },
  {
"type": "say",
"say": "api_req_started",
"ts": "2026-02-18T12:00:02Z",
"text": "{\"cost\":0.03,\"tokensIn\":10,\"tokensOut\":2,\"cacheReads\":1,\"cacheWrites\":0,\"apiProtocol\":\"openai\"}"
  }
]"#;
    let path = setup_task(&dir, "task-def", ui_messages, None);

    let messages = parse_roocode_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openai");
    assert_eq!(messages[0].model_id, "unknown");
    assert_eq!(messages[0].agent, None);
}

#[test]
fn test_parse_roocode_preserves_nested_reseller_api_protocol() {
    let dir = TempDir::new().unwrap();
    let ui_messages = r#"[
  {
"type": "say",
"say": "api_req_started",
"ts": "2026-02-18T12:00:00Z",
"text": "{\"cost\":0.12,\"tokensIn\":100,\"tokensOut\":50,\"cacheReads\":20,\"cacheWrites\":5,\"apiProtocol\":\"bedrock/anthropic\"}"
  }
]"#;
    let history = r#"before
<environment_details>
<model>claude-sonnet-4</model>
</environment_details>
after"#;
    let path = setup_task(&dir, "task-nested-provider", ui_messages, Some(history));

    let messages = parse_roocode_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "bedrock/anthropic");
}

#[test]
fn test_parse_roocode_skips_invalid_timestamp() {
    let dir = TempDir::new().unwrap();
    let ui_messages = r#"[
  {
"type": "say",
"say": "api_req_started",
"ts": "not-a-time",
"text": "{\"cost\":0.12,\"tokensIn\":100,\"tokensOut\":50,\"cacheReads\":20,\"cacheWrites\":5,\"apiProtocol\":\"anthropic\"}"
  }
]"#;
    let path = setup_task(&dir, "task-time", ui_messages, None);

    let messages = parse_roocode_file(&path);
    assert!(messages.is_empty());
}

#[test]
fn test_parse_roocode_invalid_file_json_is_ignored() {
    let dir = TempDir::new().unwrap();
    let path = setup_task(&dir, "task-invalid", "{not-json", None);

    let messages = parse_roocode_file(&path);
    assert!(messages.is_empty());
}

#[test]
fn test_extract_model_and_agent_prefers_slug_then_name() {
    let content = r#"
<environment_details>
<model>gpt-5</model>
<name>Builder</name>
</environment_details>
<environment_details>
<model>gpt-5.1</model>
<slug>reviewer</slug>
<name>Reviewer</name>
</environment_details>
"#;

    let (model, agent) = extract_model_and_agent(content);
    assert_eq!(model, "gpt-5.1");
    assert_eq!(agent.as_deref(), Some("reviewer"));
}
