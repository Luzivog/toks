use super::*;
use std::path::PathBuf;

#[test]
fn test_derive_context_from_path_extracts_channel_project_and_chat_id() {
    let p = PathBuf::from(
            "/tmp/home/.config/manicode-dev/projects/sandbox/chats/2025-12-14T10-00-00.000Z/chat-messages.json",
        );
    let (channel, project, chat_id) = derive_context_from_path(&p);
    assert_eq!(channel, "manicode-dev");
    assert_eq!(project, "sandbox");
    assert_eq!(chat_id, "2025-12-14T10-00-00.000Z");
}

#[test]
fn test_extract_assistant_usage_from_metadata_usage() {
    let msg: Value = serde_json::from_str(
        r#"{
                "role": "assistant",
                "metadata": {
                    "model": "claude-sonnet-4-20250514",
                    "usage": {
                        "inputTokens": 1000,
                        "outputTokens": 400,
                        "cacheReadInputTokens": 200,
                        "cacheCreationInputTokens": 50
                    }
                },
                "credits": 1.5
            }"#,
    )
    .unwrap();

    let usage = extract_assistant_usage(&msg);
    assert_eq!(usage.input_tokens, 1000);
    assert_eq!(usage.output_tokens, 400);
    assert_eq!(usage.cache_read_input_tokens, 200);
    assert_eq!(usage.cache_creation_input_tokens, 50);
    assert_eq!(usage.credits, 1.5);
    assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-20250514"));
}

#[test]
fn test_extract_usage_snake_case_shape() {
    let msg: Value = serde_json::from_str(
        r#"{
                "role": "assistant",
                "metadata": {
                    "codebuff": {
                        "usage": {
                            "prompt_tokens": 750,
                            "completion_tokens": 120,
                            "prompt_tokens_details": { "cached_tokens": 100 }
                        }
                    }
                }
            }"#,
    )
    .unwrap();

    let usage = extract_assistant_usage(&msg);
    assert_eq!(usage.input_tokens, 750);
    assert_eq!(usage.output_tokens, 120);
    assert_eq!(usage.cache_read_input_tokens, 100);
}

#[test]
fn test_extract_usage_falls_back_to_run_state_message_history() {
    let msg: Value = serde_json::from_str(
        r#"{
                "role": "assistant",
                "metadata": {
                    "runState": {
                        "sessionState": {
                            "mainAgentState": {
                                "messageHistory": [
                                    { "role": "user", "providerOptions": {} },
                                    {
                                        "role": "assistant",
                                        "providerOptions": {
                                            "codebuff": {
                                                "model": "openai/gpt-5",
                                                "usage": {
                                                    "inputTokens": 2000,
                                                    "outputTokens": 800,
                                                    "cacheReadInputTokens": 400
                                                }
                                            }
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }
            }"#,
    )
    .unwrap();

    let usage = extract_assistant_usage(&msg);
    assert_eq!(usage.input_tokens, 2000);
    assert_eq!(usage.output_tokens, 800);
    assert_eq!(usage.cache_read_input_tokens, 400);
    assert_eq!(usage.model.as_deref(), Some("openai/gpt-5"));
}

#[test]
fn test_is_assistant_role_accepts_variant_and_role() {
    let ai: Value = serde_json::from_str(r#"{"variant":"ai"}"#).unwrap();
    let assistant: Value = serde_json::from_str(r#"{"role":"assistant"}"#).unwrap();
    let user: Value = serde_json::from_str(r#"{"role":"user"}"#).unwrap();
    assert!(is_assistant_role(&ai));
    assert!(is_assistant_role(&assistant));
    assert!(!is_assistant_role(&user));
}

#[test]
fn test_parse_chat_id_to_millis_restores_time_separators_without_touching_date() {
    // 2025-12-14T10:00:00.000Z == 1 765 706 400 000 ms
    let expected = 1_765_706_400_000_i64;
    let parsed = parse_chat_id_to_millis("2025-12-14T10-00-00.000Z").unwrap();
    assert_eq!(parsed, expected);

    // A global `-`→`:` replace would corrupt this to "2025:12:14T..." and
    // return None. Guarding against that regression here.
    let broken = "2025-12-14T10-00-00.000Z".replace('-', ":");
    assert!(parse_timestamp_str(&broken).is_none());
}

#[test]
fn test_parse_chat_id_to_millis_returns_none_for_garbage() {
    assert!(parse_chat_id_to_millis("not-a-chat-id").is_none());
    assert!(parse_chat_id_to_millis("").is_none());
}

#[test]
fn test_parse_codebuff_file_skips_messages_without_token_signal() {
    use std::fs;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let chat_dir = dir
        .path()
        .join("manicode")
        .join("projects")
        .join("proj")
        .join("chats")
        .join("2025-12-20T12-00-00.000Z");
    fs::create_dir_all(&chat_dir).unwrap();
    let msgs_path = chat_dir.join("chat-messages.json");
    fs::write(
        &msgs_path,
        r#"[
                { "variant": "user", "content": "hi" },
                { "variant": "ai",
                  "timestamp": "2025-12-20T12:00:05.000Z",
                  "metadata": {
                    "model": "claude-sonnet-4-20250514",
                    "usage": { "inputTokens": 10, "outputTokens": 5 }
                  }
                },
                { "variant": "ai",
                  "timestamp": "2025-12-20T12:00:06.000Z",
                  "metadata": { "model": "claude-sonnet-4-20250514" }
                }
            ]"#,
    )
    .unwrap();

    let messages = parse_codebuff_file(&msgs_path);
    assert_eq!(messages.len(), 1);
    let only = &messages[0];
    assert_eq!(only.client, "codebuff");
    assert_eq!(only.model_id, "claude-sonnet-4-20250514");
    assert_eq!(only.provider_id, "anthropic");
    assert!(only.session_id.ends_with("/proj/2025-12-20T12-00-00.000Z"));
    assert_eq!(only.tokens.input, 10);
    assert_eq!(only.tokens.output, 5);
}
