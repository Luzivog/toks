use super::theme::{accent_for_model_provider, claude_accent, codex_accent, opencode_accent};

#[test]
fn model_provider_accents_keep_the_existing_groups() {
    for provider in ["anthropic", "claude", "bedrock/anthropic", "CLAUDE"] {
        assert_eq!(accent_for_model_provider(provider), claude_accent());
    }
    for provider in [
        "opencode",
        "opencodereview",
        "google",
        "gemini",
        "zenmux",
        "xai",
        "grok",
    ] {
        assert_eq!(accent_for_model_provider(provider), opencode_accent());
    }
    for provider in ["openai", "codex", "unknown-provider"] {
        assert_eq!(accent_for_model_provider(provider), codex_accent());
    }
}
