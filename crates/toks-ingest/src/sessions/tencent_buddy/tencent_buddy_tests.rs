use super::*;

#[test]
fn parse_jsonl_file_reads_assistant_usage() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("projects").join("c-Users-alice-repo");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join("session-1.jsonl");
    std::fs::write(
        &path,
        r#"{"id":"user-1","timestamp":1780000000000,"type":"message","role":"user","sessionId":"session-1","cwd":"/Users/alice/repo"}
{"id":"assistant-1","timestamp":1780000000100,"type":"message","role":"assistant","status":"completed","sessionId":"session-1","cwd":"/Users/alice/repo","providerData":{"model":"glm-5.2","messageId":"msg-1"},"message":{"usage":{"input_tokens":24486,"output_tokens":3,"total_tokens":24489,"cache_read_input_tokens":14720}}}"#,
    )
    .unwrap();

    let messages = parse_jsonl_file("codebuddy", "codebuddy", &path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "codebuddy");
    assert_eq!(message.model_id, "glm-5.2");
    // `inferred_provider_from_model` recognizes "glm" and infers "zai"
    // (Zhipu AI), taking precedence over the DEFAULT_PROVIDER ("tencent")
    // fallback — consistent with how every other model family (claude,
    // gpt, etc.) is attributed to its real provider rather than the
    // client name. DEFAULT_PROVIDER only applies when inference can't
    // identify the model at all.
    assert_eq!(message.provider_id, "zai");
    assert_eq!(message.session_id, "session-1");
    assert_eq!(message.tokens.input, 9766);
    assert_eq!(message.tokens.output, 3);
    assert_eq!(message.tokens.cache_read, 14720);
    assert_eq!(message.tokens.total(), 24489);
    assert_eq!(message.workspace_label.as_deref(), Some("repo"));
    assert_eq!(
        message.dedup_key.as_deref(),
        Some("codebuddy:session-1:msg-1")
    );
}

#[test]
fn parse_jsonl_file_keeps_ambiguous_raw_usage_input_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-2.jsonl");
    std::fs::write(
        &path,
        r#"{"id":"call-1","timestamp":1780000000100,"type":"function_call","sessionId":"session-2","providerData":{"requestModelId":"glm-5.2","messageId":"msg-2","rawUsage":{"prompt_tokens":3,"completion_tokens":2,"prompt_cache_hit_tokens":4,"prompt_cache_write_tokens":4,"completion_thinking_tokens":5}}}"#,
    )
    .unwrap();

    let messages = parse_jsonl_file("workbuddy", "workbuddy", &path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "workbuddy");
    assert_eq!(messages[0].tokens.input, 3);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 4);
    assert_eq!(messages[0].tokens.cache_write, 4);
    assert_eq!(messages[0].tokens.reasoning, 5);
    assert_eq!(messages[0].tokens.total(), 18);
}

#[test]
fn parse_jsonl_file_does_not_double_count_inclusive_cache_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-cache.jsonl");
    std::fs::write(
        &path,
        r#"{"id":"assistant-1","timestamp":1780000000100,"type":"message","role":"assistant","status":"completed","sessionId":"session-cache","cwd":"/Users/alice/repo","providerData":{"model":"glm-5.2","messageId":"msg-1"},"message":{"usage":{"input_tokens":113415,"output_tokens":990,"total_tokens":114405,"cache_read_input_tokens":112224}}}"#,
    )
    .unwrap();

    let messages = parse_jsonl_file("workbuddy", "workbuddy", &path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.tokens.input, 1191);
    assert_eq!(message.tokens.output, 990);
    assert_eq!(message.tokens.cache_read, 112224);
    // Provider-billed total (114405), not input + cache_read + output (226629).
    assert_eq!(message.tokens.total(), 114405);
}

#[test]
fn parse_jsonl_file_keeps_ambiguous_codebuddy_input_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-ambiguous.jsonl");
    std::fs::write(
        &path,
        r#"{"id":"assistant-1","timestamp":1780000000100,"type":"message","role":"assistant","status":"completed","sessionId":"session-ambiguous","cwd":"/Users/alice/repo","providerData":{"model":"glm-5.2","messageId":"msg-1"},"message":{"usage":{"inputTokens":7,"outputTokens":2,"cacheTokens":10}}}"#,
    )
    .unwrap();

    let messages = parse_jsonl_file("codebuddy", "codebuddy", &path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.tokens.input, 7);
    assert_eq!(message.tokens.output, 2);
    assert_eq!(message.tokens.cache_read, 10);
    assert_eq!(message.tokens.total(), 19);
}

#[test]
fn parse_jsonl_file_keeps_zero_cached_miss_tokens_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-zero-cache-miss.jsonl");
    std::fs::write(
        &path,
        r#"{"id":"assistant-1","timestamp":1780000000100,"type":"message","role":"assistant","status":"completed","sessionId":"session-zero-cache-miss","cwd":"/Users/alice/repo","providerData":{"model":"glm-5.2","messageId":"msg-1"},"message":{"usage":{"inputTokens":100,"outputTokens":5,"cacheTokens":100,"cachedMissTokens":0}}}"#,
    )
    .unwrap();

    let messages = parse_jsonl_file("workbuddy", "workbuddy", &path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.tokens.input, 0);
    assert_eq!(message.tokens.output, 5);
    assert_eq!(message.tokens.cache_read, 100);
    assert_eq!(message.tokens.total(), 105);
}

#[test]
fn parse_extension_log_file_splits_cached_miss_and_cache_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("moza-configurator__session.log");
    std::fs::write(
        &path,
        r#"[2026/7/1 16:56:01.100] [info] [CraftInvokableAgent] [agent-1] Model prepared: Kimi-K2.7-Code (kimi-k2.7)
[2026/7/1 16:56:02.200] [info] [AgentReporter] [agent-1] Agent execution successful with usage: {"inputTokens":140732,"outputTokens":635,"totalTokens":141367,"cacheTokens":76032,"cachedWriteTokens":0,"cachedMissTokens":64700}"#,
    )
    .unwrap();

    let messages = parse_extension_log_file("codebuddy", "codebuddy", &path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "kimi-k2.7");
    assert_eq!(messages[0].tokens.input, 64700);
    assert_eq!(messages[0].tokens.output, 635);
    assert_eq!(messages[0].tokens.cache_read, 76032);
    assert_eq!(messages[0].tokens.total(), 141367);
    assert_eq!(
        messages[0].workspace_label.as_deref(),
        Some("moza-configurator")
    );
}

#[test]
fn parse_extension_log_file_keeps_repeated_agent_usage_at_different_times() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.log");
    std::fs::write(
        &path,
        r#"[2026/7/1 16:56:01.100] [info] [CraftInvokableAgent] [agent-1] Model prepared: GLM-5.2 (glm-5.2)
[2026/7/1 16:56:02.200] [info] [AgentReporter] [agent-1] Agent execution successful with usage: {"inputTokens":10,"outputTokens":2,"totalTokens":12}
[2026/7/1 16:57:02.200] [info] [AgentReporter] [agent-1] Agent execution successful with usage: {"inputTokens":10,"outputTokens":2,"totalTokens":12}"#,
    )
    .unwrap();

    let messages = parse_extension_log_file("codebuddy", "codebuddy", &path);

    assert_eq!(messages.len(), 2);
    assert_ne!(messages[0].dedup_key, messages[1].dedup_key);
}

#[test]
fn parse_extension_log_file_mirrored_sinks_share_dedup_key_despite_ms_skew() {
    // The same agent execution is logged to the extension's own sink and to
    // the host's output-channel sink; each writer stamps its own prefix a few
    // ms apart (and in a different format). Both copies must produce the same
    // dedup key so cross-file dedup collapses them.
    let dir = tempfile::tempdir().unwrap();

    let extension_sink = dir.path().join("proj__session.log");
    std::fs::write(
        &extension_sink,
        r#"[2026/7/1 16:56:02.200] [info] [AgentReporter] [agent-1] Agent execution successful with usage: {"inputTokens":140732,"outputTokens":635,"totalTokens":141367}"#,
    )
    .unwrap();

    let host_sink = dir.path().join("proj__host.log");
    std::fs::write(
        &host_sink,
        r#"2026-07-01 16:56:02.201 [info] [AgentReporter] [agent-1] Agent execution successful with usage: {"inputTokens":140732,"outputTokens":635,"totalTokens":141367}"#,
    )
    .unwrap();

    let from_extension = parse_extension_log_file("codebuddy", "codebuddy", &extension_sink);
    let from_host = parse_extension_log_file("codebuddy", "codebuddy", &host_sink);

    assert_eq!(from_extension.len(), 1);
    assert_eq!(from_host.len(), 1);
    assert!(from_extension[0].dedup_key.is_some());
    assert_eq!(from_extension[0].dedup_key, from_host[0].dedup_key);
}
