use super::create_lookup;

// =========================================================================
// OPENCODE ZEN MODELS - GPT-5 FAMILY
// All models from https://opencode.ai/docs/zen/
// =========================================================================

#[test]
fn test_opencode_zen_gpt_5_2() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.2").unwrap();
    assert_eq!(result.matched_key, "gpt-5.2");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gpt_5_1() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.1").unwrap();
    assert_eq!(result.matched_key, "gpt-5.1");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gpt_5_1_codex() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.1-codex").unwrap();
    assert_eq!(result.matched_key, "gpt-5.1-codex");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gpt_5_1_codex_max() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.1-codex-max").unwrap();
    assert_eq!(result.matched_key, "gpt-5.1-codex-max");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gpt_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5").unwrap();
    assert_eq!(result.matched_key, "gpt-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gpt_5_codex() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5-codex").unwrap();
    assert_eq!(result.matched_key, "gpt-5-codex");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gpt_5_nano() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5-nano").unwrap();
    assert_eq!(result.matched_key, "gpt-5-nano");
    assert_eq!(result.source, "LiteLLM");
}

// =========================================================================
// OPENCODE ZEN MODELS - CLAUDE FAMILY
// =========================================================================

#[test]
fn test_opencode_zen_claude_sonnet_4_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4-5").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_claude_sonnet_4() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4").unwrap();
    assert_eq!(result.matched_key, "anthropic/claude-sonnet-4");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_claude_haiku_4_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-haiku-4-5").unwrap();
    assert_eq!(result.matched_key, "claude-haiku-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_claude_3_5_haiku_with_dot() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-3.5-haiku").unwrap();
    assert_eq!(result.matched_key, "anthropic/claude-3.5-haiku");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_claude_opus_4_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-opus-4-5").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_claude_opus_4_1() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-opus-4-1").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-1");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_glm_4_6() {
    let lookup = create_lookup();
    let result = lookup.lookup("glm-4.6").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.6");
    assert_eq!(result.source, "OpenRouter");
}

// =========================================================================
// OPENCODE ZEN MODELS - KIMI FAMILY
// =========================================================================

#[test]
fn test_opencode_zen_kimi_k2() {
    let lookup = create_lookup();
    let result = lookup.lookup("kimi-k2").unwrap();
    assert_eq!(result.matched_key, "moonshotai/kimi-k2");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_kimi_k2_thinking() {
    let lookup = create_lookup();
    let result = lookup.lookup("kimi-k2-thinking").unwrap();
    assert_eq!(result.matched_key, "moonshotai/kimi-k2-thinking");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_opencode_zen_kimi_k2_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("kimi-k2.5").unwrap();
    assert_eq!(result.matched_key, "moonshotai/kimi-k2.5");
    assert_eq!(result.source, "OpenRouter");
}

// =========================================================================
// OPENCODE ZEN MODELS - QWEN FAMILY
// =========================================================================

#[test]
fn test_opencode_zen_qwen3_coder() {
    let lookup = create_lookup();
    let result = lookup.lookup("qwen3-coder").unwrap();
    assert_eq!(result.matched_key, "qwen/qwen3-coder");
    assert_eq!(result.source, "OpenRouter");
}

// =========================================================================
// BASELINE / LEGACY TESTS
// =========================================================================

#[test]
fn test_exact_match_litellm() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-4o").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_exact_match_gpt_5_5_litellm() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.5").unwrap();
    assert_eq!(result.matched_key, "gpt-5.5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_exact_match_openrouter() {
    let lookup = create_lookup();
    let result = lookup.lookup("z-ai/glm-4.7").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.7");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_openrouter_model_part_match() {
    let lookup = create_lookup();
    let result = lookup.lookup("glm-4.7").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.7");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_force_source_litellm() {
    let lookup = create_lookup();
    let result = lookup
        .lookup_with_source("gpt-4o", Some("litellm"))
        .unwrap();
    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.matched_key, "gpt-4o");
}

#[test]
fn test_force_source_openrouter() {
    let lookup = create_lookup();
    let result = lookup
        .lookup_with_source("gpt-4o", Some("openrouter"))
        .unwrap();
    assert_eq!(result.source, "OpenRouter");
    assert_eq!(result.matched_key, "openai/gpt-4o");
}

#[test]
fn test_case_insensitive() {
    let lookup = create_lookup();
    let result = lookup.lookup("GPT-4O").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
}
