use super::super::*;
use super::create_lookup;

#[test]
fn test_tier_suffix_low() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.1-codex-low").unwrap();
    assert_eq!(result.matched_key, "gpt-5.1-codex");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_tier_suffix_high() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-4o-high").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_tier_suffix_free() {
    let lookup = create_lookup();
    let result = lookup.lookup("glm-4.7-free").unwrap();
    assert_eq!(result.matched_key, "z-ai/glm-4.7");
    assert_eq!(result.source, "OpenRouter");
}

#[test]
fn test_tier_suffix_xhigh() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.2-xhigh").unwrap();
    assert_eq!(result.matched_key, "gpt-5.2");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_tier_suffix_xhigh_gpt_5_5() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.5-xhigh").unwrap();
    assert_eq!(result.matched_key, "gpt-5.5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_tier_suffix_xhigh_codex_max() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.1-codex-max-xhigh").unwrap();
    assert_eq!(result.matched_key, "gpt-5.1-codex-max");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_parenthesized_reasoning_tier_gpt_levels() {
    let lookup = create_lookup();

    for tier in ["minimal", "low", "medium", "high", "xhigh", "auto", "none"] {
        let id = format!("gpt-5.2({tier})");
        let result = lookup.lookup(&id).unwrap_or_else(|| panic!("{id} miss"));
        assert_eq!(result.matched_key, "gpt-5.2", "{id}");
        assert_eq!(result.source, "LiteLLM", "{id}");
    }
}

#[test]
fn test_parenthesized_reasoning_tier_claude_and_gemini() {
    let lookup = create_lookup();

    let claude = lookup.lookup("claude-sonnet-4-5(high)").unwrap();
    assert_eq!(claude.matched_key, "claude-sonnet-4-5");
    assert_eq!(claude.source, "LiteLLM");

    // Dot-form claude id (cliproxyapi accepts either) routes through
    // version-separator normalization to the dashed catalog entry.
    let claude_dot = lookup.lookup("claude-sonnet-4.5(none)").unwrap();
    assert_eq!(claude_dot.matched_key, "claude-sonnet-4-5");

    let gemini = lookup.lookup("gemini-3-pro(auto)").unwrap();
    assert_eq!(gemini.matched_key, "openrouter/google/gemini-3-pro-preview");
}

#[test]
fn test_parenthesized_reasoning_tier_with_routing_prefix() {
    let lookup = create_lookup();

    let prefixed = lookup.lookup("myproxy-gpt-5.2(xhigh)").unwrap();
    assert_eq!(prefixed.matched_key, "gpt-5.2");

    let antigravity = lookup
        .lookup("antigravity-claude-sonnet-4-5(high)")
        .unwrap();
    assert_eq!(antigravity.matched_key, "claude-sonnet-4-5");
}

#[test]
fn test_parenthesized_reasoning_tier_unknown_value_does_not_strip() {
    let lookup = create_lookup();

    // Values outside the cliproxyapi level set must not silently
    // misresolve via `try_strip_unknown_suffix`: without an early
    // return, splitting on `-` would peel the parenthesized fragment
    // off and match a shorter, unrelated model id (e.g.
    // `gpt-5.2-codex(invalid)` collapsing to `gpt-5.2`).
    assert!(lookup.lookup("gpt-5.2(weirdgarbage)").is_none());
    assert!(lookup.lookup("gpt-5.2(1024)").is_none());
    assert!(lookup.lookup("gpt-5.2()").is_none());
    assert!(lookup.lookup("gpt-5.2-codex(invalid)").is_none());
    assert!(lookup.lookup("myproxy-gpt-5.2(invalid)").is_none());

    // The same guard must hold across model families so that the
    // generalized stripper never misresolves a non-GPT id by peeling
    // a parenthesized fragment off through the dash-suffix path.
    assert!(lookup
        .lookup("antigravity-claude-sonnet-4-5(invalid)")
        .is_none());
    assert!(lookup.lookup("claude-sonnet-4-5(garbage)").is_none());
    assert!(lookup.lookup("gemini-3-pro(weird)").is_none());
}

#[test]
fn test_parenthesized_reasoning_tier_cost_matches_base_model() {
    let lookup = create_lookup();
    let base = lookup.calculate_cost("gpt-5.2", 1_000_000, 500_000, 0, 0, 0);
    let tiered = lookup.calculate_cost("gpt-5.2(xhigh)", 1_000_000, 500_000, 0, 0, 0);

    assert!((tiered - base).abs() < f64::EPSILON);
    assert!((tiered - 8.75).abs() < 0.001);
}

// =========================================================================
// INTELLIGENT PREFIX/SUFFIX STRIPPING TESTS
// =========================================================================

#[test]
fn test_antigravity_prefix_gemini_3_flash() {
    let lookup = create_lookup();
    let result = lookup.lookup("antigravity-gemini-3-flash").unwrap();
    assert_eq!(result.matched_key, "vertex_ai/gemini-3-flash-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_antigravity_prefix_gemini_3_pro() {
    let lookup = create_lookup();
    let result = lookup.lookup("antigravity-gemini-3-pro").unwrap();
    assert_eq!(result.matched_key, "openrouter/google/gemini-3-pro-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_antigravity_prefix_with_tier_suffix() {
    let lookup = create_lookup();
    let result = lookup.lookup("antigravity-gemini-3-pro-high").unwrap();
    assert_eq!(result.matched_key, "openrouter/google/gemini-3-pro-preview");
}

#[test]
fn test_antigravity_prefix_claude() {
    let lookup = create_lookup();
    let result = lookup.lookup("antigravity-claude-sonnet-4-5").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_antigravity_prefix_gpt() {
    let lookup = create_lookup();
    let result = lookup.lookup("antigravity-gpt-4o").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_antigravity_prefix_case_insensitive() {
    let lookup = create_lookup();
    let result = lookup.lookup("Antigravity-gpt-4o").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
}

#[test]
fn test_antigravity_cost_calculation() {
    let lookup = create_lookup();
    let cost_with_prefix =
        lookup.calculate_cost("antigravity-gpt-5.2", 1_000_000, 500_000, 0, 0, 0);
    let cost_without_prefix = lookup.calculate_cost("gpt-5.2", 1_000_000, 500_000, 0, 0, 0);
    assert!((cost_with_prefix - cost_without_prefix).abs() < 0.001);
    assert!(cost_with_prefix > 0.0);
}

// New tests for intelligent detection

#[test]
fn test_unknown_prefix_generic() {
    let lookup = create_lookup();
    let result = lookup.lookup("myplugin-gpt-4o").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
}

#[test]
fn test_unknown_prefix_two_segments() {
    let lookup = create_lookup();
    let result = lookup.lookup("router-v2-claude-sonnet-4-5").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
}

#[test]
fn test_unknown_suffix_thinking() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4-5-thinking").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
}

#[test]
fn test_unknown_suffix_two_segments() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-opus-4-5-thinking-pro").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-5");
}

#[test]
fn test_prefix_and_suffix_combined() {
    let lookup = create_lookup();
    let result = lookup
        .lookup("antigravity-claude-opus-4-5-thinking")
        .unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-5");
}

#[test]
fn test_prefix_and_suffix_with_tier() {
    let lookup = create_lookup();
    let result = lookup
        .lookup("antigravity-claude-opus-4-5-thinking-high")
        .unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-5");
}

#[test]
fn test_no_false_positive_valid_model() {
    let lookup = create_lookup();
    // gpt-4o-mini is a valid model, should NOT strip "gpt"
    let result = lookup.lookup("gpt-4o-mini").unwrap();
    assert_eq!(result.matched_key, "gpt-4o-mini");
}

#[test]
fn test_suffix_strip_high() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4-5-high").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
}

#[test]
fn test_suffix_strip_xhigh() {
    let lookup = create_lookup();
    let result = lookup.lookup("claude-sonnet-4-5-xhigh").unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-5");
}

#[test]
fn test_suffix_strip_low() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-4o-low").unwrap();
    assert_eq!(result.matched_key, "gpt-4o");
}

#[test]
fn test_suffix_strip_codex() {
    let lookup = create_lookup();
    let result = lookup.lookup("gpt-5.2-codex").unwrap();
    assert_eq!(result.matched_key, "gpt-5.2");
}

/// Regression (#1062): a bare router label must not be priced from a
/// coincidence of spelling. `auto` used to elect `morph/auto` at
/// $0.85/$1.55 — an unrelated code-apply vendor — and submit at those
/// rates, because covers_usage only demands rates for populated buckets.
#[test]
fn bare_routing_labels_do_not_resolve_but_qualified_ones_do() {
    let mut models_dev = HashMap::new();
    for key in ["morph/auto", "llmgateway/auto", "cursor/agent_review"] {
        models_dev.insert(
            key.to_string(),
            ModelPricing {
                input_cost_per_token: Some(8.5e-7),
                output_cost_per_token: Some(1.55e-6),
                ..Default::default()
            },
        );
    }
    let lookup = PricingLookup::new_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        models_dev,
    );

    // Five parsers emit these bare; nothing records the real model.
    assert!(lookup.lookup("auto").is_none());
    assert!(lookup.lookup("AUTO").is_none());
    assert!(lookup.lookup("agent_review").is_none());

    // A tier suffix does not make it a model: this normalizes to `auto`
    // before the model-part fallback runs.
    assert!(lookup.lookup("auto(high)").is_none());
    // Nor does an unrecognized vendor prefix, which is dropped to retry
    // the bare id. A real `morph/auto` never reaches that fallback.
    assert!(lookup.lookup("cx/auto").is_none());

    // A qualified id names a real vendor's model and still prices.
    assert!(lookup.lookup("morph/auto").is_some());
}
