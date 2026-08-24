use super::super::*;
use super::create_lookup;

#[test]
fn test_opencode_zen_kimi_k2_6_provider_hint_from_kimi_for_coding() {
    let lookup = create_lookup();
    let result = lookup
        .lookup_with_provider("k2p6", Some("kimi-for-coding"))
        .unwrap();
    assert_eq!(result.matched_key, "moonshotai/kimi-k2.6");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_provider_hint_prefers_matching_pricing_source() {
    let lookup = create_lookup();
    let result = lookup
        .lookup_with_provider("grok-code", Some("azure"))
        .unwrap();
    assert_eq!(result.matched_key, "azure_ai/grok-code-fast-1");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_provider_hint_matches_nested_reseller_exact_key() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    litellm.insert(
        "azure/openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup.lookup_with_provider("gpt-4", Some("azure")).unwrap();
    assert_eq!(result.matched_key, "azure/openai/gpt-4");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_provider_hint_normalizes_openai_codex_alias() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-5.2-preview".into(),
        ModelPricing {
            input_cost_per_token: Some(1.0),
            ..Default::default()
        },
    );
    litellm.insert(
        "google/gpt-5.2-preview-max".into(),
        ModelPricing {
            input_cost_per_token: Some(2.0),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup
        .lookup_with_provider("gpt-5.2", Some("openai-codex"))
        .unwrap();
    assert_eq!(result.matched_key, "openai/gpt-5.2-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_provider_scoped_path_does_not_strip_into_wrong_fireworks_model() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fireworks_ai/accounts/fireworks/models/deepseek-r1-0528-distill-qwen3-8b".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000002),
            output_cost_per_token: Some(0.0000002),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    assert!(
        lookup
            .lookup("accounts/fireworks/models/deepseek-v4-pro")
            .is_none(),
        "provider-scoped model paths should not be shortened into unrelated fuzzy matches"
    );
}

#[test]
fn test_provider_scoped_path_matches_exact_litellm_reseller_key() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fireworks_ai/accounts/fireworks/models/deepseek-v4-pro".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000003),
            output_cost_per_token: Some(0.0000004),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup
        .lookup("accounts/fireworks/models/deepseek-v4-pro")
        .unwrap();

    assert_eq!(
        result.matched_key,
        "fireworks_ai/accounts/fireworks/models/deepseek-v4-pro"
    );
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_provider_scoped_path_matches_exact_terminal_provider_key() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fireworks_ai/deepseek-v4-pro".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0000003),
            output_cost_per_token: Some(0.0000004),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup
        .lookup("accounts/fireworks/models/deepseek-v4-pro")
        .unwrap();

    assert_eq!(result.matched_key, "fireworks_ai/deepseek-v4-pro");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_provider_scoped_path_does_not_use_upstream_openrouter_exact() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "deepseek/deepseek-v4-pro".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000001),
            output_cost_per_token: Some(0.000002),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(HashMap::new(), openrouter, HashMap::new());

    assert!(
        lookup
            .lookup("accounts/fireworks/models/deepseek-v4-pro")
            .is_none(),
        "Fireworks-scoped usage should not be priced with upstream DeepSeek rates"
    );
}

/// Regression (#831): router/proxy-assigned ids like `cx/gpt-5.5` (seen
/// from OpenCode's `omniroute` provider) carry a prefix outside the
/// curated `PROVIDER_PREFIXES` list, so the pricing lookup used to return
/// `None` (and thus bill $0) instead of stripping the prefix and pricing
/// the underlying `gpt-5.5` model.
#[test]
fn test_unknown_prefixed_model_id_strips_to_underlying_model() {
    let lookup = create_lookup();
    let direct = lookup.lookup("gpt-5.5").unwrap();
    let prefixed = lookup.lookup("cx/gpt-5.5").unwrap();
    assert_eq!(prefixed.matched_key, direct.matched_key);
    assert_eq!(prefixed.source, direct.source);
    assert_eq!(
        prefixed.pricing.input_cost_per_token,
        direct.pricing.input_cost_per_token
    );
    assert_eq!(
        prefixed.pricing.output_cost_per_token,
        direct.pricing.output_cost_per_token
    );
}

/// Regression (#846): an id carrying both a routing prefix and a tier
/// suffix resolved to nothing, so real usage billed $0. Each id below
/// resolves once one transformation is applied, but the two were never
/// applied together: prefix stripping only retried the terminal segment
/// as-is, and suffix stripping splits on `-`, so it never shed the `cx/`.
#[test]
fn test_routing_prefix_and_tier_suffix_strip_together() {
    let lookup = create_lookup();
    let expected = lookup.lookup("gpt-5.5").unwrap();

    for id in [
        "cx/gpt-5.5-xhigh",
        "cx/gpt-5.5-high",
        "cx/gpt-5.5-medium",
        "cx/gpt-5.5-low",
    ] {
        let result = lookup
            .lookup(id)
            .unwrap_or_else(|| panic!("{id} must resolve"));
        assert_eq!(result.matched_key, expected.matched_key, "id: {id}");
        assert_eq!(
            result.pricing.input_cost_per_token, expected.pricing.input_cost_per_token,
            "id: {id}"
        );
    }
}

/// Regression (#831): an id with an unrecognized provider prefix AND an
/// unrecognized underlying model must still return `None` rather than
/// fuzzy-matching something unrelated.
#[test]
fn test_unknown_prefixed_unknown_model_stays_none() {
    let lookup = create_lookup();
    assert!(lookup.lookup("unknown/nonexistent").is_none());
}
