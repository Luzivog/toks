use super::*;

#[test]
fn test_normalize_model_name_custom_prefix() {
    // TypeScript keeps trailing digits: "claude-opus-4-5-thinking-0"
    assert_eq!(
        normalize_model_name("custom:Claude-Opus-4.5-Thinking-[Anthropic]-0"),
        "claude-opus-4-5-thinking-0"
    );
}

#[test]
fn test_normalize_model_name_simple() {
    // Dots become hyphens: "gemini-2.5-pro" -> "gemini-2-5-pro"
    assert_eq!(normalize_model_name("gemini-2.5-pro"), "gemini-2-5-pro");
}

#[test]
fn test_normalize_model_name_brackets() {
    // TypeScript keeps trailing digits: "claude-sonnet-4"
    assert_eq!(
        normalize_model_name("Claude-Sonnet-4-[Anthropic]"),
        "claude-sonnet-4"
    );
}

#[test]
fn test_get_provider_from_model() {
    let provider_for = |model| provider_from_model_or(model, "unknown");
    assert_eq!(provider_for("claude-3-sonnet"), "anthropic");
    assert_eq!(provider_for("opus-4"), "anthropic");
    assert_eq!(provider_for("sonnet-4"), "anthropic");
    assert_eq!(provider_for("haiku-3"), "anthropic");
    assert_eq!(provider_for("gpt-4o"), "openai");
    assert_eq!(provider_for("o1-preview"), "openai");
    assert_eq!(provider_for("o3-mini"), "openai");
    assert_eq!(provider_for("gemini-pro"), "google");
    assert_eq!(provider_for("grok-2"), "xai");
    assert_eq!(provider_for("unknown-model"), "unknown");
}

#[test]
fn test_get_default_model_from_provider() {
    assert_eq!(
        get_default_model_from_provider("anthropic"),
        "claude-unknown"
    );
    assert_eq!(get_default_model_from_provider("openai"), "gpt-unknown");
    assert_eq!(get_default_model_from_provider("google"), "gemini-unknown");
    assert_eq!(get_default_model_from_provider("xai"), "grok-unknown");
    assert_eq!(get_default_model_from_provider("custom"), "custom-unknown");
}

#[test]
fn test_parse_droid_settings_structure() {
    let json = r#"{
            "model": "custom:Claude-Opus-4.5-Thinking-[Anthropic]-0",
            "providerLock": "anthropic",
            "providerLockTimestamp": "2024-12-26T12:00:00Z",
            "tokenUsage": {
                "inputTokens": 1234,
                "outputTokens": 567,
                "cacheCreationTokens": 89,
                "cacheReadTokens": 12,
                "thinkingTokens": 34
            }
        }"#;

    let mut bytes = json.as_bytes().to_vec();
    let settings: DroidSettingsJson = simd_json::from_slice(&mut bytes).unwrap();

    assert_eq!(
        settings.model,
        Some("custom:Claude-Opus-4.5-Thinking-[Anthropic]-0".to_string())
    );
    assert_eq!(settings.provider_lock, Some("anthropic".to_string()));

    let usage = settings.token_usage.unwrap();
    assert_eq!(usage.input_tokens, Some(1234));
    assert_eq!(usage.output_tokens, Some(567));
    assert_eq!(usage.cache_creation_tokens, Some(89));
    assert_eq!(usage.cache_read_tokens, Some(12));
    assert_eq!(usage.thinking_tokens, Some(34));
}
