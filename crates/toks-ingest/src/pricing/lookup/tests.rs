use super::*;

mod best_match_selection_endpoint_aliases;
mod best_match_selection_pricing;
mod best_match_selection_sources;
mod best_match_selection_vendor_roots;
mod cost_computation_openai;
mod cost_computation_tiers;
mod exact_matching;
mod normalization_catalog_and_versions;
mod normalization_guards;
mod prefix_fuzzy_matching_guards;
mod prefix_fuzzy_matching_stripping;
mod provider_prefix_hints;
mod provider_prefix_scoped_path_matching;
mod provider_prefix_tier_selection;

/// Mock LiteLLM data matching real API responses for OpenCode Zen models
fn mock_litellm() -> HashMap<String, ModelPricing> {
    let mut m = HashMap::new();

    // === GPT-4 models (baseline) ===
    m.insert(
        "gpt-4o".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000025),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(0.00000125),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-4o-mini".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000015),
            output_cost_per_token: Some(0.0000006),
            cache_read_input_token_cost: Some(0.000000075),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-4-turbo".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00003),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    // === OpenCode Zen: GPT-5 family ===
    m.insert(
        "gpt-5.2".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000175),
            output_cost_per_token: Some(0.000014),
            cache_read_input_token_cost: Some(1.75e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5.5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            input_cost_per_token_above_272k_tokens: Some(0.000010),
            output_cost_per_token: Some(0.000030),
            output_cost_per_token_above_272k_tokens: Some(0.000045),
            cache_read_input_token_cost: Some(0.0000005),
            cache_read_input_token_cost_above_272k_tokens: Some(0.000001),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5.1".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5.1-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5.1-codex-max".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "gpt-5-nano".into(),
        ModelPricing {
            input_cost_per_token: Some(5e-8),
            output_cost_per_token: Some(4e-7),
            cache_read_input_token_cost: Some(5e-9),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    // === OpenCode Zen: Claude family (LiteLLM entries) ===
    m.insert(
        "claude-3-5-sonnet-20241022".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_read_input_token_cost: Some(0.0000003),
            cache_creation_input_token_cost: Some(0.00000375),
            ..Default::default()
        },
    );
    m.insert(
        "claude-sonnet-4-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_read_input_token_cost: Some(3e-7),
            cache_creation_input_token_cost: Some(0.00000375),
            ..Default::default()
        },
    );
    m.insert(
        "claude-haiku-4-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000001),
            output_cost_per_token: Some(0.000005),
            cache_read_input_token_cost: Some(1e-7),
            cache_creation_input_token_cost: Some(0.00000125),
            ..Default::default()
        },
    );
    m.insert(
        "bedrock/us.anthropic.claude-3-5-haiku-20241022-v1:0".into(),
        ModelPricing {
            input_cost_per_token: Some(8e-7),
            output_cost_per_token: Some(0.000004),
            cache_read_input_token_cost: Some(8e-8),
            cache_creation_input_token_cost: Some(0.000001),
            ..Default::default()
        },
    );
    m.insert(
        "claude-opus-4-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            cache_read_input_token_cost: Some(5e-7),
            cache_creation_input_token_cost: Some(0.00000625),
            ..Default::default()
        },
    );
    m.insert(
        "claude-opus-4-1".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_read_input_token_cost: Some(0.0000015),
            cache_creation_input_token_cost: Some(0.00001875),
            ..Default::default()
        },
    );

    // === OpenCode Zen: Gemini family (LiteLLM entries) ===
    m.insert(
        "openrouter/google/gemini-3-pro-preview".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000002),
            output_cost_per_token: Some(0.000012),
            cache_read_input_token_cost: Some(2e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "vertex_ai/gemini-3-flash-preview".into(),
        ModelPricing {
            input_cost_per_token: Some(5e-7),
            output_cost_per_token: Some(0.000003),
            cache_read_input_token_cost: Some(5e-8),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    // === OpenCode Zen: Grok (LiteLLM entry) ===
    m.insert(
        "xai/grok-code-fast-1-0825".into(),
        ModelPricing {
            input_cost_per_token: Some(2e-7),
            output_cost_per_token: Some(0.0000015),
            cache_read_input_token_cost: Some(2e-8),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    m.insert(
        "azure_ai/grok-code-fast-1".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000035),
            output_cost_per_token: Some(0.0000175),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "bedrock/anthropic.claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_read_input_token_cost: Some(3e-7),
            cache_creation_input_token_cost: Some(0.00000375),
            ..Default::default()
        },
    );
    m.insert(
        "vertex_ai/gemini-2.5-pro".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.000005),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "google/gemini-2.5-pro".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.000005),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    m
}

/// Mock OpenRouter data matching real API responses for OpenCode Zen models
fn mock_openrouter() -> HashMap<String, ModelPricing> {
    let mut m = HashMap::new();

    // === Baseline models ===
    m.insert(
        "openai/gpt-4o".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000025),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(0.00000125),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    // === OpenCode Zen: Claude (OpenRouter entries) ===
    m.insert(
        "anthropic/claude-sonnet-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_read_input_token_cost: Some(3e-7),
            cache_creation_input_token_cost: Some(0.00000375),
            ..Default::default()
        },
    );
    m.insert(
        "anthropic/claude-opus-4-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            cache_read_input_token_cost: Some(0.0000005),
            cache_creation_input_token_cost: Some(0.00000625),
            ..Default::default()
        },
    );
    m.insert(
        "anthropic/claude-3.5-haiku".into(),
        ModelPricing {
            input_cost_per_token: Some(8e-7),
            output_cost_per_token: Some(0.000004),
            cache_read_input_token_cost: Some(8e-8),
            cache_creation_input_token_cost: Some(0.000001),
            ..Default::default()
        },
    );

    // === OpenCode Zen: GLM family ===
    m.insert(
        "z-ai/glm-4.7".into(),
        ModelPricing {
            input_cost_per_token: Some(4e-7),
            output_cost_per_token: Some(0.0000015),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "z-ai/glm-4.6".into(),
        ModelPricing {
            input_cost_per_token: Some(3.9e-7),
            output_cost_per_token: Some(0.0000019),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    m.insert(
        "moonshotai/kimi-k2".into(),
        ModelPricing {
            input_cost_per_token: Some(4.56e-7),
            output_cost_per_token: Some(0.00000184),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "moonshotai/kimi-k2.5".into(),
        ModelPricing {
            input_cost_per_token: Some(4.5e-7),
            output_cost_per_token: Some(0.0000025),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "moonshotai/kimi-k2.6".into(),
        ModelPricing {
            input_cost_per_token: Some(9.5e-7),
            output_cost_per_token: Some(0.000004),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    m.insert(
        "moonshotai/kimi-k2-thinking".into(),
        ModelPricing {
            input_cost_per_token: Some(4e-7),
            output_cost_per_token: Some(0.00000175),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    // === OpenCode Zen: Qwen family ===
    m.insert(
        "qwen/qwen3-coder".into(),
        ModelPricing {
            input_cost_per_token: Some(2.2e-7),
            output_cost_per_token: Some(9.5e-7),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    m
}

fn create_lookup() -> PricingLookup {
    PricingLookup::new(mock_litellm(), mock_openrouter(), HashMap::new())
}
