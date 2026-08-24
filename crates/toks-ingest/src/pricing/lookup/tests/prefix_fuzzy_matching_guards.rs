use super::super::*;
use super::create_lookup;

// =========================================================================
// OPENCODE ZEN MODELS - GLM FAMILY
// =========================================================================

#[test]
fn test_opencode_zen_glm_4_7_free() {
    let lookup = create_lookup();
    let result = lookup.lookup("glm-4.7-free").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.7");
    assert_eq!(result.source, "OpenRouter");
}

// =========================================================================
// OPENCODE ZEN MODELS - GEMINI FAMILY
// =========================================================================

#[test]
fn test_opencode_zen_gemini_3_pro() {
    let lookup = create_lookup();
    let result = lookup.lookup("gemini-3-pro").unwrap();
    assert_eq!(result.matched_key, "openrouter/google/gemini-3-pro-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_gemini_3_flash() {
    let lookup = create_lookup();
    let result = lookup.lookup("gemini-3-flash").unwrap();
    assert_eq!(result.matched_key, "vertex_ai/gemini-3-flash-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_opencode_zen_kimi_k2_5_free() {
    let lookup = create_lookup();
    let result = lookup.lookup("kimi-k2.5-free").unwrap();
    assert_eq!(result.matched_key, "moonshotai/kimi-k2.5");
    assert_eq!(result.source, "OpenRouter");
}

// =========================================================================
// OPENCODE ZEN MODELS - GROK FAMILY
// =========================================================================

#[test]
fn test_opencode_zen_grok_code() {
    let lookup = create_lookup();
    let result = lookup.lookup("grok-code").unwrap();
    assert_eq!(result.matched_key, "xai/grok-code-fast-1-0825");
    assert_eq!(result.source, "LiteLLM");
}

// Regression: a generic id whose only fuzzy-eligible remnant after suffix
// stripping is the bare word `model` (real example seen in local data:
// `model-zero-usage-v1`, `test-model`) must NOT fuzzy-match a real priced
// key like `azure_ai/model_router`. The word `model` carries no model
// identity and is on the FUZZY_BLOCKLIST.
#[test]
fn fuzzy_match_does_not_resolve_generic_model_token() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "azure_ai/model_router".into(),
        ModelPricing {
            input_cost_per_token: Some(1.4e-7),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    // The bare token must not resolve.
    assert!(lookup.lookup("model").is_none());
    // Ids that strip down to the bare `model` token must not misresolve.
    assert!(lookup.lookup("model-zero-usage-v1").is_none());
    assert!(lookup.lookup("model-nonzero-usage-v1").is_none());
    assert!(lookup.lookup("test-model").is_none());

    // But an EXACT key match is still honored — `model-router` is a real
    // model id, not a fuzzy remnant.
    let mut litellm2 = HashMap::new();
    litellm2.insert(
        "azure/model-router".into(),
        ModelPricing {
            input_cost_per_token: Some(1.4e-7),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    let lookup2 = PricingLookup::new(litellm2, HashMap::new(), HashMap::new());
    assert_eq!(
        lookup2.lookup("model-router").unwrap().matched_key,
        "azure/model-router"
    );
}

// Regression: `gemini-default` is a generic routing label — it names which
// router served the request, never which model did — so it must stay
// unpriced and be excluded from submission. Its fuzzy-eligible remnant
// after prefix stripping is the bare word `default`, which substring-hits
// LiteLLM's real `fireworks-ai-default` row.
//
// That row is priced 0.0/0.0, and `covers_usage` counts an explicit zero as
// a real rate, so before `default` joined the FUZZY_BLOCKLIST the label
// looked priced and `exclude_unpriced_submission_messages` let it
// through — a Google routing label submitted at Fireworks AI's rates.
// Verified against the live LiteLLM dataset: `fireworks-ai-default` is a
// real key with input and output cost 0.0.
#[test]
fn fuzzy_match_does_not_resolve_generic_default_token() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fireworks-ai-default".into(),
        ModelPricing {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    // The bare token must not resolve.
    assert!(lookup.lookup("default").is_none());
    // Nor the routing label that strips down to it, with or without the
    // provider hint the submission path passes.
    assert!(lookup.lookup("gemini-default").is_none());
    assert!(lookup
        .lookup_with_provider("gemini-default", Some("google"))
        .is_none());

    // But an EXACT key match is still honored — `fireworks-ai-default` is a
    // real id in the dataset, not a fuzzy remnant.
    assert_eq!(
        lookup.lookup("fireworks-ai-default").unwrap().matched_key,
        "fireworks-ai-default"
    );
}

// The blocklist is consulted with the *query* remnant, so blocking
// `default` must not stop a query from matching INTO a dataset key that
// merely ends in `@default`. LiteLLM ships seven of those
// (`vertex_ai/claude-*@default`), and they are ordinary priced models.
#[test]
fn blocking_the_default_token_still_matches_vertex_default_suffixed_keys() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "vertex_ai/claude-opus-4-7@default".into(),
        ModelPricing {
            input_cost_per_token: Some(5e-06),
            output_cost_per_token: Some(2.5e-05),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    assert_eq!(
        lookup
            .lookup("vertex_ai/claude-opus-4-7@default")
            .unwrap()
            .matched_key,
        "vertex_ai/claude-opus-4-7@default"
    );
    assert_eq!(
        lookup
            .lookup("claude-opus-4-7@default")
            .unwrap()
            .matched_key,
        "vertex_ai/claude-opus-4-7@default"
    );
}

// Defense-in-depth beyond #1070: the resolver-top `is_routing_label`
// guard refuses the router labels parsers emit today (`auto`,
// `agent_review`), but the model-part index is a second, deeper place a
// bare id can elect another provider's row. Any provider may publish a
// generic `FUZZY_BLOCKLIST` token as a model part (`default`, `router`,
// `mini`, ...) — none do today, but a bare id carrying such a token names
// no model, so it must not land on whatever unrelated key shares the
// spelling. This guard covers shapes the label list does not enumerate;
// full dataset keys still resolve.
#[test]
fn model_part_index_does_not_resolve_bare_generic_tokens() {
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "someprovider/router".into(),
        ModelPricing {
            input_cost_per_token: Some(1e-6),
            output_cost_per_token: Some(2e-6),
            ..Default::default()
        },
    );
    models_dev.insert(
        "someprovider/default".into(),
        ModelPricing {
            input_cost_per_token: Some(1e-6),
            output_cost_per_token: Some(2e-6),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        models_dev,
    );

    // A bare generic token must not resolve through another provider's
    // model part.
    assert!(lookup.lookup("router").is_none());
    assert!(lookup.lookup("default").is_none());

    // The tokens' own full dataset keys are still exact matches.
    assert_eq!(
        lookup.lookup("someprovider/router").unwrap().matched_key,
        "someprovider/router"
    );
    assert_eq!(
        lookup.lookup("someprovider/default").unwrap().matched_key,
        "someprovider/default"
    );
}

#[test]
fn test_blocklist_auto() {
    let lookup = create_lookup();
    assert!(lookup.lookup("auto").is_none());
}

#[test]
fn test_blocklist_mini() {
    let lookup = create_lookup();
    assert!(lookup.lookup("mini").is_none());
}

#[test]
fn test_fuzzy_match_gemini() {
    let lookup = create_lookup();
    let result = lookup.lookup("gemini-3-pro").unwrap();
    assert_eq!(result.matched_key, "openrouter/google/gemini-3-pro-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_tier_suffix_with_fuzzy() {
    let lookup = create_lookup();
    let result = lookup.lookup("gemini-3-pro-high").unwrap();
    assert_eq!(result.matched_key, "openrouter/google/gemini-3-pro-preview");
}

#[test]
fn test_nonexistent_model() {
    let lookup = create_lookup();
    assert!(lookup.lookup("nonexistent-model-xyz").is_none());
}

#[test]
fn test_fallback_suffix_lookup() {
    // Create a lookup with only the base model (no -codex variant)
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    // Note: gpt-5-codex is NOT in the pricing data

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    // Looking up gpt-5-codex should fall back to gpt-5
    let result = lookup.lookup("gpt-5-codex").unwrap();
    assert_eq!(result.matched_key, "gpt-5");
    assert_eq!(result.source, "LiteLLM");

    // Looking up gpt-5-codex-max should also fall back to gpt-5
    let result = lookup.lookup("gpt-5-codex-max").unwrap();
    assert_eq!(result.matched_key, "gpt-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_fallback_suffix_with_tier_suffix() {
    // Test that tier suffix + fallback suffix both work together
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: Some(1.25e-7),
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    // gpt-5-codex-high should strip -high first, then fall back from gpt-5-codex to gpt-5
    let result = lookup.lookup("gpt-5-codex-high").unwrap();
    assert_eq!(result.matched_key, "gpt-5");
    assert_eq!(result.source, "LiteLLM");

    // gpt-5-codex-max-xhigh should strip -xhigh first, then fall back from gpt-5-codex-max to gpt-5
    let result = lookup.lookup("gpt-5-codex-max-xhigh").unwrap();
    assert_eq!(result.matched_key, "gpt-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_fallback_suffix_prefers_exact_match() {
    // If the exact model exists, it should be used (no fallback)
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-5".into(),
        ModelPricing {
            input_cost_per_token: Some(0.00000125),
            output_cost_per_token: Some(0.00001),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );
    litellm.insert(
        "gpt-5-codex".into(),
        ModelPricing {
            input_cost_per_token: Some(0.000002), // Different price to verify which one is used
            output_cost_per_token: Some(0.000015),
            cache_read_input_token_cost: None,
            cache_creation_input_token_cost: None,
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    // Should use the exact match, not fall back
    let result = lookup.lookup("gpt-5-codex").unwrap();
    assert_eq!(result.matched_key, "gpt-5-codex");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.000002));
}

#[test]
fn test_is_fuzzy_eligible() {
    assert!(!is_fuzzy_eligible("auto"));
    assert!(!is_fuzzy_eligible("mini"));
    assert!(!is_fuzzy_eligible("chat"));
    assert!(!is_fuzzy_eligible("base"));
    assert!(!is_fuzzy_eligible("abc"));
    assert!(is_fuzzy_eligible("gpt-4o"));
    // Bare brand tokens carry no model information: a fuzzy hit from them
    // can land on any model of the brand, so they are blocklisted.
    assert!(!is_fuzzy_eligible("claude"));
    assert!(!is_fuzzy_eligible("anthropic"));
}
