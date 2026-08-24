use super::*;
#[test]
fn test_normalize_model_for_grouping() {
    assert_eq!(
        normalize_model_for_grouping("claude-opus-4-5-20251101"),
        "claude-opus-4-5"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-sonnet-4-5-20250929"),
        "claude-sonnet-4-5"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-sonnet-4-20250514"),
        "claude-sonnet-4"
    );

    assert_eq!(
        normalize_model_for_grouping("claude-opus-4.5"),
        "claude-opus-4-5"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-sonnet-4.5"),
        "claude-sonnet-4-5"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-opus-4.6"),
        "claude-opus-4-6"
    );
    assert_eq!(
        normalize_model_for_grouping("anthropic/claude-4-6-sonnet"),
        "claude-sonnet-4-6"
    );
    assert_eq!(
        normalize_model_for_grouping("anthropic/claude-4-5-haiku"),
        "claude-haiku-4-5"
    );
    assert_eq!(
        normalize_model_for_grouping("anthropic/claude-4-6-opus"),
        "claude-opus-4-6"
    );

    assert_eq!(normalize_model_for_grouping("gpt-5.2"), "gpt-5.2");
    assert_eq!(normalize_model_for_grouping("gpt-5.4(xhigh)"), "gpt-5.4");
    assert_eq!(normalize_model_for_grouping("gpt-5.4(high)"), "gpt-5.4");
    assert_eq!(normalize_model_for_grouping("gpt-5.4(minimal)"), "gpt-5.4");
    assert_eq!(normalize_model_for_grouping("gpt-5.4(auto)"), "gpt-5.4");
    assert_eq!(normalize_model_for_grouping("gpt-5.4(none)"), "gpt-5.4");
    assert_eq!(
        normalize_model_for_grouping("gpt-5.4(weirdgarbage)"),
        "gpt-5.4(weirdgarbage)"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-sonnet-4.5(high)"),
        "claude-sonnet-4-5"
    );
    assert_eq!(
        normalize_model_for_grouping("gemini-3-pro(auto)"),
        "gemini-3-pro"
    );
    assert_eq!(
        normalize_model_for_grouping("gemini-2.5-pro"),
        "gemini-2.5-pro"
    );

    assert_eq!(
        normalize_model_for_grouping("claude-opus-4-5-high"),
        "claude-opus-4-5-high"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-opus-4-5-thinking-high"),
        "claude-opus-4-5-thinking-high"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-sonnet-4-5-high"),
        "claude-sonnet-4-5-high"
    );

    assert_eq!(
        normalize_model_for_grouping("claude-4-sonnet"),
        "claude-4-sonnet"
    );
    assert_eq!(
        normalize_model_for_grouping("claude-4-opus-thinking"),
        "claude-4-opus-thinking"
    );

    assert_eq!(normalize_model_for_grouping("big-pickle"), "big-pickle");
    assert_eq!(normalize_model_for_grouping("grok-code"), "grok-code");

    assert_eq!(
        normalize_model_for_grouping("claude-opus-4.5-20251101"),
        "claude-opus-4-5"
    );
}
