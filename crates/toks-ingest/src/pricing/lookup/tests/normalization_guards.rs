use super::super::normalize::{normalize_model_name, normalize_version_separator};
use super::super::*;

// =========================================================================
// Generalized Claude family/major/minor normalization (PR #634 rework)
// =========================================================================

/// Synthetic dataset mirroring real LiteLLM/OpenRouter key shapes, with
/// deliberately adversarial gaps: bedrock-style `us.anthropic.` keys exist
/// for opus but not sonnet, and OpenRouter carries a pricier opus `-fast`
/// variant that the old fallbacks degraded other families onto.
fn claude_family_fixture() -> PricingLookup {
    fn p(input: f64, output: f64) -> ModelPricing {
        ModelPricing {
            input_cost_per_token: Some(input),
            output_cost_per_token: Some(output),
            ..Default::default()
        }
    }

    let mut litellm = HashMap::new();
    litellm.insert("claude-opus-4".to_string(), p(15e-6, 75e-6));
    litellm.insert("claude-opus-4-1".to_string(), p(15e-6, 75e-6));
    litellm.insert("claude-opus-4-5".to_string(), p(5e-6, 25e-6));
    litellm.insert("claude-opus-4-6".to_string(), p(5e-6, 25e-6));
    litellm.insert("claude-opus-4-7".to_string(), p(5e-6, 25e-6));
    litellm.insert("claude-opus-4-8".to_string(), p(5e-6, 25e-6));
    litellm.insert("claude-sonnet-4".to_string(), p(3e-6, 15e-6));
    litellm.insert("claude-sonnet-4-5".to_string(), p(3e-6, 15e-6));
    litellm.insert("claude-sonnet-4-6".to_string(), p(3e-6, 15e-6));
    litellm.insert("claude-haiku-4-5".to_string(), p(1e-6, 5e-6));
    litellm.insert("us.anthropic.claude-opus-4-8".to_string(), p(5e-6, 25e-6));
    litellm.insert("vertex_ai/claude-sonnet-4-6".to_string(), p(3e-6, 15e-6));

    let mut openrouter = HashMap::new();
    openrouter.insert("anthropic/claude-opus-4".to_string(), p(15e-6, 75e-6));
    openrouter.insert("anthropic/claude-opus-4.8".to_string(), p(5e-6, 25e-6));
    openrouter.insert("anthropic/claude-opus-4.8-fast".to_string(), p(7e-6, 30e-6));
    openrouter.insert("anthropic/claude-sonnet-4.6".to_string(), p(3e-6, 15e-6));
    openrouter.insert("anthropic/claude-haiku-4.5".to_string(), p(1e-6, 5e-6));
    openrouter.insert("anthropic/claude-fable-5".to_string(), p(5e-6, 25e-6));

    PricingLookup::new(litellm, openrouter, HashMap::new())
}

#[test]
fn test_normalize_minor_generalizes_across_families() {
    assert_eq!(
        normalize_model_name("claude-sonnet-4-7"),
        Some("claude-sonnet-4-7".into())
    );
    assert_eq!(
        normalize_model_name("sonnet-4.7"),
        Some("claude-sonnet-4-7".into())
    );
    assert_eq!(
        normalize_model_name("claude-haiku-4-6"),
        Some("claude-haiku-4-6".into())
    );
    assert_eq!(
        normalize_model_name("haiku-4.6"),
        Some("claude-haiku-4-6".into())
    );
    assert_eq!(
        normalize_model_name("claude-opus-4-9"),
        Some("claude-opus-4-9".into())
    );
    assert_eq!(
        normalize_model_name("opus-4.9"),
        Some("claude-opus-4-9".into())
    );
    assert_eq!(
        normalize_model_name("opus-5-2"),
        Some("claude-opus-5-2".into())
    );
}

#[test]
fn test_normalize_reversed_order_all_families() {
    assert_eq!(
        normalize_model_name("claude-4-8-opus"),
        Some("claude-opus-4-8".into())
    );
    assert_eq!(
        normalize_model_name("4-8-opus"),
        Some("claude-opus-4-8".into())
    );
    assert_eq!(
        normalize_model_name("claude-4-6-sonnet"),
        Some("claude-sonnet-4-6".into())
    );
    assert_eq!(
        normalize_model_name("claude-4-5-haiku"),
        Some("claude-haiku-4-5".into())
    );
}

#[test]
fn test_normalize_bare_modern_major() {
    assert_eq!(
        normalize_model_name("claude-sonnet-5"),
        Some("claude-sonnet-5".into())
    );
    assert_eq!(
        normalize_model_name("claude-opus-5"),
        Some("claude-opus-5".into())
    );
    assert_eq!(
        normalize_model_name("fable-5"),
        Some("claude-fable-5".into())
    );
    assert_eq!(
        normalize_model_name("claude-fable-5[1m]"),
        Some("claude-fable-5".into())
    );
}

/// Boundary contract preserved from main's hardcoded matcher: two-digit
/// minors and majors, zero minors, undelimited versions, and dated forms
/// must not normalize to a coarser key. (PR #634's original parser
/// degraded `opus-4-60` to `claude-opus-4`; main's contract is None.)
#[test]
fn test_normalize_modern_claude_boundaries() {
    assert_eq!(normalize_model_name("opus-4-60"), None);
    assert_eq!(normalize_model_name("sonnet-4-60"), None);
    assert_eq!(normalize_model_name("opus-14-6"), None);
    assert_eq!(normalize_model_name("opus4"), None);
    assert_eq!(normalize_model_name("opus-4x"), None);
    assert_eq!(normalize_model_name("opus-3"), None);
    assert_eq!(normalize_model_name("claude-sonnet-5-0"), None);
    assert_eq!(normalize_model_name("claude-opus-4-20250514"), None);
}

/// Legacy 3.x ids keep their irregular canonical keys; the reversed-order
/// and bare-major parsing must not hijack the digit pairs in them.
#[test]
fn test_normalize_legacy_line_not_hijacked_by_modern_parser() {
    assert_eq!(
        normalize_model_name("claude-3-5-sonnet"),
        Some("claude-3.5-sonnet".into())
    );
    assert_eq!(
        normalize_model_name("claude-3-7-sonnet-20250219"),
        Some("claude-3-7-sonnet".into())
    );
    assert_eq!(
        normalize_model_name("claude-3-5-haiku-20241022"),
        Some("claude-3.5-haiku".into())
    );
}

/// Regression (B1): a bedrock-style sonnet id must never be billed at an
/// opus key. Before the family guard, `us.anthropic.claude-sonnet-4-6-v1:0`
/// suffix-stripped down to `us.anthropic.claude` and fuzzy-matched the
/// dataset's `us.anthropic.claude-opus-4-8` entry ($5/M instead of $3/M).
#[test]
fn test_bedrock_sonnet_never_billed_as_opus() {
    let lookup = claude_family_fixture();
    let result = lookup
        .lookup("us.anthropic.claude-sonnet-4-6-v1:0")
        .unwrap();
    assert_eq!(result.matched_key, "claude-sonnet-4-6");
    assert_eq!(result.pricing.input_cost_per_token, Some(3e-6));
}

/// Regression (B2): reversed-order sonnet ids must resolve to the sonnet
/// key, not cross-family. Before reversed-order parsing was generalized
/// beyond opus, `claude-4-6-sonnet` stripped down to `claude` and
/// fuzzy-matched `anthropic/claude-opus-4.8-fast`.
#[test]
fn test_reversed_sonnet_resolves_canonical_not_cross_family() {
    let lookup = claude_family_fixture();
    for id in ["claude-4-6-sonnet", "4-6-sonnet"] {
        let result = lookup.lookup(id).unwrap();
        assert_eq!(result.matched_key, "claude-sonnet-4-6", "id: {id}");
    }
    let result = lookup.lookup("claude-4-5-haiku").unwrap();
    assert_eq!(result.matched_key, "claude-haiku-4-5");
}

/// Regression (B3): the never-degrade contract that
/// `test_unknown_future_opus_minor_does_not_degrade_to_opus_4` pins for
/// opus now holds for sonnet and haiku too. Unknown minors previously
/// degraded: `sonnet-4-7` -> claude-sonnet-4.6, `haiku-4-6` ->
/// claude-haiku-4.5 (and with real data even claude-3.5-haiku).
#[test]
fn test_unknown_sonnet_haiku_minor_does_not_degrade() {
    let lookup = claude_family_fixture();
    for id in [
        "sonnet-4-7",
        "claude-sonnet-4-7",
        "sonnet-4-60",
        "haiku-4-6",
        "claude-haiku-4-6",
    ] {
        assert!(lookup.lookup(id).is_none(), "id {id} must not degrade");
    }
}

/// Regression (B4): major >= 5 ids resolve to a dataset-known exact id
/// when one exists, else None — never to a different major. Previously
/// `claude-opus-5` resolved to `anthropic/claude-opus-4.8-fast` and
/// `sonnet-5`/`claude-sonnet-5-0` to sonnet 4.6, while bare `opus-5`
/// happened to return None only because of a fuzzy length cutoff.
#[test]
fn test_major_five_never_resolves_to_different_major() {
    let lookup = claude_family_fixture();
    for id in [
        "claude-opus-5",
        "opus-5",
        "opus-5-2",
        "sonnet-5",
        "claude-sonnet-5-0",
    ] {
        assert!(
            lookup.lookup(id).is_none(),
            "id {id} must not resolve to a 4.x key"
        );
    }

    // fable-5 is dataset-known (OpenRouter) and resolves in all forms.
    for id in [
        "claude-fable-5",
        "fable-5",
        "claude-fable-5[1m]",
        "anthropic/claude-fable-5",
    ] {
        let result = lookup.lookup(id).unwrap();
        assert_eq!(result.matched_key, "anthropic/claude-fable-5", "id: {id}");
    }
}

/// Regression (#831): a dataset key that legitimately keeps its own
/// provider prefix (e.g. `anthropic/claude-fable-5`, which exists as its
/// own OpenRouter key) must still resolve via the exact/direct lookup —
/// the new generic prefix-stripping fallback must not preempt it.
#[test]
fn test_known_prefixed_dataset_key_still_resolves_exactly() {
    let lookup = claude_family_fixture();
    let result = lookup.lookup("anthropic/claude-fable-5").unwrap();
    assert_eq!(result.matched_key, "anthropic/claude-fable-5");
}

/// When the dataset later gains a major-5 key, the same ids resolve to it
/// with no code change — the "known version" decision is dataset-driven.
#[test]
fn test_major_five_resolves_once_dataset_knows_it() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-opus-5".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.00001),
            output_cost_per_token: Some(0.00005),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    for id in ["claude-opus-5", "opus-5", "aws.claude-opus-5-thinking"] {
        let result = lookup.lookup(id).unwrap();
        assert_eq!(result.matched_key, "claude-opus-5", "id: {id}");
    }
}

/// Known minors keep resolving across the id shapes seen in the wild:
/// dotted versions, vendor prefixes, tier/feature suffixes.
#[test]
fn test_known_minor_shapes_resolve_per_family() {
    let lookup = claude_family_fixture();
    let cases = [
        ("opus-4-8", "claude-opus-4-8"),
        ("opus-4.8", "claude-opus-4-8"),
        ("aws.claude-opus-4-8", "claude-opus-4-8"),
        ("claude-opus-4-8-thinking", "claude-opus-4-8"),
        ("claude-sonnet-4-6", "claude-sonnet-4-6"),
        ("claude-sonnet-4.6", "claude-sonnet-4-6"),
        ("sonnet-4-6", "claude-sonnet-4-6"),
        ("sonnet-4.6", "claude-sonnet-4-6"),
        ("aws.claude-sonnet-4-6-v1", "claude-sonnet-4-6"),
        ("claude-sonnet-4-6-thinking", "claude-sonnet-4-6"),
        ("haiku-4-5", "claude-haiku-4-5"),
        ("haiku-4.5", "claude-haiku-4-5"),
        ("vertex_ai/claude-sonnet-4-6", "vertex_ai/claude-sonnet-4-6"),
    ];
    for (id, expected) in cases {
        let result = lookup.lookup(id).unwrap();
        assert_eq!(result.matched_key, expected, "id: {id}");
    }
}

/// Ported from PR #634: the next opus minor must prefer its own key over
/// the bare `claude-opus-4` catch-all, in dashed and dotted forms.
#[test]
fn test_normalize_opus_4_8_prefers_4_8_over_4() {
    let lookup = claude_family_fixture();
    for id in ["opus-4-8", "opus-4.8"] {
        let result = lookup.lookup(id).unwrap();
        assert_eq!(result.matched_key, "claude-opus-4-8", "id: {id}");
        assert_eq!(result.source, "LiteLLM");
    }
}

/// Ported from PR #634: `aws.claude-opus-4-8` must not degrade to
/// OpenRouter's legacy `anthropic/claude-opus-4` (~3x overcharge).
#[test]
fn test_aws_opus_4_8_does_not_degrade_to_opus_4() {
    let lookup = claude_family_fixture();
    let result = lookup.lookup("aws.claude-opus-4-8").unwrap();
    assert_eq!(result.matched_key, "claude-opus-4-8");

    // 8.4M input + 873K output at opus-4-8 rates is ~$64, not ~$191
    // (legacy opus 4 at $15/$75 per M).
    let cost = lookup.calculate_cost("aws.claude-opus-4-8", 8_400_000, 873_000, 0, 0, 0);
    assert!(
        (60.0..=70.0).contains(&cost),
        "expected opus-4-8 priced cost around $64, got ${cost:.2}"
    );
}

/// Regression (post-#634 catalog audit, bug 1): retired `claude-2.x` ids
/// (present in historical usage logs, absent from every pricing dataset)
/// must resolve to None, not to a modern model's price. Previously
/// `try_strip_unknown_suffix` eroded `claude-2.1` to bare `claude`
/// (the "2.1" segment failed the all-digits version check), which then
/// fuzzy-matched `anthropic/claude-opus-4.7-fast` at $30/$150. The #634
/// family veto was bypassed because `claude-2.1` carries no
/// opus/sonnet/haiku/fable token.
#[test]
fn claude_2x_never_fuzzy_matches_modern_models() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4.7-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(30e-6),
            output_cost_per_token: Some(150e-6),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(HashMap::new(), openrouter, HashMap::new());

    for id in ["claude-2.1", "claude-2.0", "claude", "anthropic"] {
        assert!(
            lookup.lookup(id).is_none(),
            "id {id} must resolve unpriced, never to another model's price"
        );
    }
}

/// Positive control for the claude-2.x guards: when a dataset actually
/// prices `claude-2.1`, it still resolves — the guards only block the
/// erosion-to-bare-brand path, not legitimate dataset hits.
#[test]
fn claude_2x_still_resolves_when_dataset_prices_it() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "claude-2.1".to_string(),
        ModelPricing {
            input_cost_per_token: Some(8e-6),
            output_cost_per_token: Some(24e-6),
            ..Default::default()
        },
    );
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4.7-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(30e-6),
            output_cost_per_token: Some(150e-6),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());

    let result = lookup.lookup("claude-2.1").unwrap();
    assert_eq!(result.matched_key, "claude-2.1");
    assert_eq!(result.pricing.input_cost_per_token, Some(8e-6));
}

#[test]
fn test_normalize_version_separator() {
    assert_eq!(
        normalize_version_separator("glm-4-7"),
        Some("glm-4.7".into())
    );
    assert_eq!(
        normalize_version_separator("glm-4-6"),
        Some("glm-4.6".into())
    );
    assert_eq!(
        normalize_version_separator("claude-3-5-haiku"),
        Some("claude-3.5-haiku".into())
    );
    assert_eq!(
        normalize_version_separator("gpt-5-1-codex"),
        Some("gpt-5.1-codex".into())
    );
    assert_eq!(normalize_version_separator("gpt-4o"), None);
    assert_eq!(normalize_version_separator("claude-sonnet"), None);
    assert_eq!(normalize_version_separator("big-pickle"), None);
}

#[test]
fn test_normalize_version_separator_preserves_dates() {
    assert_eq!(normalize_version_separator("2024-11-20"), None);
    assert_eq!(normalize_version_separator("model-2024-11-20"), None);
    assert_eq!(
        normalize_version_separator("claude-3-5-sonnet-20241022"),
        Some("claude-3.5-sonnet-20241022".into())
    );
    assert_eq!(normalize_version_separator("sonnet-20241022"), None);
    assert_eq!(normalize_version_separator("model-20241022-v1"), None);
}
