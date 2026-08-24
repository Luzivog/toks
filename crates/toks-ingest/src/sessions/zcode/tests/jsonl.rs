use super::*;

#[test]
fn test_parse_with_authoritative_usage() {
    let dir = TempDir::new().unwrap();
    let jsonl = format!(
        "{}\n{}",
        json!({
            "role": "user",
            "sessionId": "s1",
            "timestamp": "2026-06-20T10:00:00Z",
            "content": "hello"
        }),
        json!({
            "role": "assistant",
            "sessionId": "s1",
            "timestamp": "2026-06-20T10:00:05Z",
            "model": "glm-5.2",
            "content": "Hi there!",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "input_cache_read": 20
            }
        }),
    );
    let path = write_session(&dir, "proj", "s1", &jsonl);
    let messages = parse_zcode_file(&path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.client, "zcode");
    assert_eq!(msg.provider_id, "zhipu");
    assert_eq!(msg.model_id, "glm-5.2");
    assert_eq!(msg.session_id, "s1");
    assert_eq!(msg.tokens.input, 100);
    assert_eq!(msg.tokens.output, 50);
    assert_eq!(msg.tokens.cache_read, 20);
    assert!(msg.is_turn_start);
}

#[test]
fn test_parse_with_estimated_tokens() {
    let dir = TempDir::new().unwrap();
    let user_content = json!([{"type": "text", "text": "12345678"}]);
    let asst_content = json!([{"type": "text", "text": "abcd"}]);
    let jsonl = format!(
        "{}\n{}",
        json!({"role": "user", "sessionId": "s2", "content": user_content}),
        json!({"role": "assistant", "sessionId": "s2", "content": asst_content}),
    );
    let path = write_session(&dir, "repo", "s2", &jsonl);
    let messages = parse_zcode_file(&path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.model_id, "glm-5.2"); // default
    assert!(msg.tokens.input > 0);
    assert!(msg.tokens.output > 0);
    assert_eq!(msg.tokens.cache_read, 0);
}

#[test]
fn test_canonicalize_model() {
    assert_eq!(canonicalize_model("GLM-5.2"), "glm-5.2");
    assert_eq!(canonicalize_model("GLM-5-Turbo"), "glm-5-turbo");
    assert_eq!(canonicalize_model("glm-5.2"), "glm-5.2");
}

#[test]
fn test_content_chars_treats_empty_string_as_empty() {
    // Empty string content must count as 0 chars, consistent with null,
    // empty array, and empty object — otherwise serializing `""` yields 2
    // chars and produces a spurious estimated token.
    assert_eq!(content_chars(&json!("")), 0);
    assert_eq!(content_chars(&serde_json::Value::Null), 0);
    assert_eq!(content_chars(&json!([])), 0);
    assert_eq!(content_chars(&json!({})), 0);
    assert!(content_chars(&json!("abcd")) > 0);
}

#[test]
fn test_empty_string_assistant_content_emits_no_message() {
    // An assistant entry with empty-string content and no token usage has
    // nothing to estimate, so it must take the zero-token continue path
    // instead of emitting a fake 1-token message.
    let dir = TempDir::new().unwrap();
    let jsonl = format!(
        "{}\n{}",
        json!({"role": "user", "sessionId": "s", "content": ""}),
        json!({"role": "assistant", "sessionId": "s", "content": ""}),
    );
    let path = write_session(&dir, "proj", "s", &jsonl);
    let messages = parse_zcode_file(&path);

    assert!(messages.is_empty());
}

#[test]
fn test_usage_with_alternative_field_names() {
    let dir = TempDir::new().unwrap();
    let jsonl = format!(
        "{}\n{}",
        json!({"role": "user", "sessionId": "s3", "content": "hi"}),
        json!({
            "role": "assistant",
            "sessionId": "s3",
            "content": "bye",
            "token_usage": {
                "prompt_tokens": 200,
                "completion_tokens": 100
            }
        }),
    );
    let path = write_session(&dir, "p", "s3", &jsonl);
    let messages = parse_zcode_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.output, 100);
}

#[test]
fn test_cumulative_context_estimation() {
    let dir = TempDir::new().unwrap();
    let jsonl = concat!(
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"aaaa"}]}"#,
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"bbbb"}]}"#,
        "\n",
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"cccc"}]}"#,
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"dddd"}]}"#,
    );
    let path = write_session(&dir, "proj", "s", jsonl);
    let messages = parse_zcode_file(&path);

    assert_eq!(messages.len(), 2);
    assert!(messages[1].tokens.input > messages[0].tokens.input);
}

#[test]
fn test_model_switch_mid_session() {
    let dir = TempDir::new().unwrap();
    let jsonl = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        json!({"role": "user", "sessionId": "s", "content": "hi"}),
        json!({
            "role": "assistant",
            "sessionId": "s",
            "model": "GLM-5.2",
            "content": "first",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }),
        json!({"role": "user", "sessionId": "s", "content": "switch"}),
        json!({
            "role": "assistant",
            "sessionId": "s",
            "model": "glm-5-turbo",
            "content": "second",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }),
        json!({"role": "user", "sessionId": "s", "content": "again"}),
        json!({
            "role": "assistant",
            "sessionId": "s",
            "content": "third",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }),
    );
    let path = write_session(&dir, "proj", "s", &jsonl);
    let messages = parse_zcode_file(&path);

    assert_eq!(messages.len(), 3);
    // Each assistant message reflects the model in effect at that point.
    assert_eq!(messages[0].model_id, "glm-5.2");
    assert_eq!(messages[1].model_id, "glm-5-turbo");
    assert_ne!(messages[0].model_id, messages[1].model_id);
    // An entry with no `model` field inherits the most-recently-seen model.
    assert_eq!(messages[2].model_id, "glm-5-turbo");
}

#[test]
fn test_empty_usage_falls_back_to_token_usage() {
    let dir = TempDir::new().unwrap();
    let jsonl = format!(
        "{}\n{}",
        json!({"role": "user", "sessionId": "s", "content": "hi"}),
        json!({
            "role": "assistant",
            "sessionId": "s",
            "content": "bye",
            "usage": {},
            "token_usage": {
                "input_tokens": 321,
                "output_tokens": 123,
                "input_cache_read": 7
            }
        }),
    );
    let path = write_session(&dir, "p", "s", &jsonl);
    let messages = parse_zcode_file(&path);

    assert_eq!(messages.len(), 1);
    // Authoritative token_usage counts are used, NOT estimated.
    assert_eq!(messages[0].tokens.input, 321);
    assert_eq!(messages[0].tokens.output, 123);
    assert_eq!(messages[0].tokens.cache_read, 7);
}
