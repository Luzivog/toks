use super::super::select::{is_original_provider, is_reseller_provider};
use super::super::*;
use super::create_lookup;

// =========================================================================
// PROVIDER PREFERENCE TESTS
// =========================================================================

#[test]
fn test_provider_preference_grok_prefers_xai_over_azure() {
    let lookup = create_lookup();
    let result = lookup.lookup("grok-code").unwrap();
    assert_eq!(result.matched_key, "xai/grok-code-fast-1-0825");
    assert_eq!(result.source, "LiteLLM");
    assert!(!result.matched_key.starts_with("azure"));
}

/// Test that documents the exact before/after behavior for grok-code provider preference.
/// This test explicitly verifies that the original provider (xai/) is preferred over resellers (azure_ai/).
#[test]
fn test_grok_code_prefers_xai_over_azure() {
    // =========================================================================
    // BEFORE FIX: grok-code → azure_ai/grok-code-fast-1 ($3.50/$17.50) ❌ reseller
    // AFTER FIX:  grok-code → xai/grok-code-fast-1-0825 ($0.20/$1.50) ✅ original provider
    //
    // The azure_ai/ prefix indicates a reseller (Azure AI marketplace), which typically
    // has higher prices. The xai/ prefix indicates the original provider (X.AI/Grok),
    // which offers lower direct pricing. Our lookup should prefer the original provider.
    // =========================================================================

    let mut litellm = HashMap::new();

    // Reseller entry: azure_ai/ prefix with higher prices ($3.50/$17.50 per 1M tokens)
    litellm.insert(
        "azure_ai/grok-code-fast-1".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000035),  // $3.50/1M tokens
            output_cost_per_token: Some(0.0000175), // $17.50/1M tokens
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    // Original provider entry: xai/ prefix with lower prices ($0.20/$1.50 per 1M tokens)
    litellm.insert(
        "xai/grok-code-fast-1-0825".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000002),  // $0.20/1M tokens
            output_cost_per_token: Some(0.0000015), // $1.50/1M tokens
            cache_read_input_token_cost: Some(0.00000002),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("grok-code").unwrap();

    // Must prefer xai (original provider) over azure_ai (reseller)
    assert!(
        result.matched_key.starts_with("xai/"),
        "Expected xai/ prefix (original provider) but got: {}. \
         The lookup should prefer original providers over resellers.",
        result.matched_key
    );
    assert_eq!(
        result.matched_key, "xai/grok-code-fast-1-0825",
        "Should match the xai/grok-code-fast-1-0825 entry, not azure_ai/grok-code-fast-1"
    );

    // Verify we got the lower price (original provider)
    let pricing = &result.pricing;
    assert!(
        pricing.input_cost_per_token.unwrap() < 0.000001,
        "Input cost should be ~$0.20/1M (0.0000002), not ~$3.50/1M (reseller price)"
    );
    assert!(
        pricing.output_cost_per_token.unwrap() < 0.000005,
        "Output cost should be ~$1.50/1M (0.0000015), not ~$17.50/1M (reseller price)"
    );
}

#[test]
fn test_provider_preference_gemini_prefers_google_over_vertex() {
    let lookup = create_lookup();
    let result = lookup.lookup("gemini-2.5-pro").unwrap();
    assert_eq!(result.matched_key, "google/gemini-2.5-pro");
    assert_eq!(result.source, "LiteLLM");
    assert!(!result.matched_key.starts_with("vertex_ai"));
}

#[test]
fn test_is_original_provider() {
    assert!(is_original_provider("xai/grok-code"));
    assert!(is_original_provider("anthropic/claude-3"));
    assert!(is_original_provider("openai/gpt-4"));
    assert!(is_original_provider("google/gemini"));
    assert!(is_original_provider("x-ai/grok"));
    assert!(!is_original_provider("azure_ai/grok"));
    assert!(!is_original_provider("bedrock/anthropic"));
    assert!(!is_original_provider("vertex_ai/gemini"));
    assert!(!is_original_provider("unknown-provider/model"));
}

#[test]
fn test_is_reseller_provider() {
    assert!(is_reseller_provider("azure_ai/grok-code"));
    assert!(is_reseller_provider("azure/openai/gpt-4"));
    assert!(is_reseller_provider("bedrock/anthropic.claude"));
    assert!(is_reseller_provider("vertex_ai/gemini"));
    assert!(is_reseller_provider("together_ai/llama"));
    assert!(is_reseller_provider("groq/llama"));
    assert!(is_reseller_provider("orcarouter/openai/gpt-4"));
    assert!(!is_reseller_provider("xai/grok"));
    assert!(!is_reseller_provider("anthropic/claude"));
    assert!(!is_reseller_provider("openai/gpt-4"));
}

/// Regression test for #336: subscription-based resellers (e.g. Perplexity) with
/// all-None pricing should not shadow valid entries during provider-aware lookup.
/// `perplexity/anthropic/claude-opus-4-6` matches provider hint "anthropic" via
/// its path segments, but has no per-token pricing. The lookup must fall through
/// to the exact `claude-opus-4-6` entry that has real pricing data.
#[test]
fn test_none_pricing_reseller_does_not_shadow_real_entry() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            cache_read_input_token_cost: Some(0.0000005),
            cache_creation_input_token_cost: Some(0.00000625),
            ..Default::default()
        },
    );
    // Perplexity entry: matches "anthropic" hint but has no pricing
    litellm.insert(
        "perplexity/anthropic/claude-opus-4-6".into(),
        ModelPricing::default(),
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    // With provider hint "anthropic", should find the real entry, not perplexity
    let result = lookup.lookup_with_provider("claude-opus-4-6", Some("anthropic"));
    assert!(result.is_some(), "lookup should succeed");
    let result = result.unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-6");
    assert!(result.pricing.input_cost_per_token.is_some());

    // Cost should be non-zero
    let cost = lookup.calculate_cost("claude-opus-4-6", 100_000, 50_000, 0, 0, 0);
    assert!(cost > 0.0, "cost should be positive, got {}", cost);
}

#[test]
fn test_none_pricing_provider_match_falls_back_to_priced_fuzzy_candidate() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-4-6-20250301".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            ..Default::default()
        },
    );
    litellm.insert(
        "perplexity/anthropic/claude-opus-4-6-20250301".into(),
        ModelPricing::default(),
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let result = lookup.lookup_with_provider("claude-opus-4-6-latest", Some("anthropic"));
    assert!(result.is_some(), "lookup should succeed via fuzzy fallback");
    let result = result.unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-6-20250301"); // gitleaks:allow
    assert_eq!(result.source, "LiteLLM");
    assert!(result.pricing.input_cost_per_token.is_some());
}

#[test]
fn test_none_pricing_exact_litellm_does_not_shadow_openrouter_model_part() {
    let mut litellm = HashMap::new();
    litellm.insert("claude-opus-4-6".into(), ModelPricing::default());

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4-6".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000005),
            output_cost_per_token: Some(0.000025),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let result = lookup.lookup("claude-opus-4-6").unwrap();

    assert_eq!(result.source, "OpenRouter");
    assert_eq!(result.matched_key, "anthropic/claude-opus-4-6");

    let cost = lookup.calculate_cost("claude-opus-4-6", 100, 20, 0, 0, 0);
    assert!(cost > 0.0, "cost should use priced fallback, got {cost}");
}

#[test]
fn test_none_pricing_provider_exact_does_not_shadow_stripped_priced_entry() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "anthropic/claude-sonnet-4-5".into(),
        ModelPricing::default(),
    );
    litellm.insert(
        "claude-sonnet-4-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("anthropic/claude-sonnet-4-5").unwrap();

    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.matched_key, "claude-sonnet-4-5");

    let cost = lookup.calculate_cost("anthropic/claude-sonnet-4-5", 100, 20, 0, 0, 0);
    assert!(
        cost > 0.0,
        "cost should use stripped priced entry, got {cost}"
    );
}

#[test]
fn test_zero_pricing_exact_entry_is_usable() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "free-model".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup("free-model").unwrap();

    assert_eq!(result.matched_key, "free-model");
    assert_eq!(lookup.calculate_cost("free-model", 100, 20, 0, 0, 0), 0.0);
}
