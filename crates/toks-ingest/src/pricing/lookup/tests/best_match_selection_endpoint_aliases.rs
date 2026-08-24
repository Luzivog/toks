use super::super::*;

/// Direct Vertex hints must keep Vertex's hosted pricing even when an
/// Anthropic first-party row is also available. The canonical provider tag
/// makes both candidates reachable; the raw hint decides which root owns
/// the usage.
#[test]
fn direct_vertex_hint_outranks_anthropic_first_party_alias() {
    for vertex_root in ["vertex", "vertex_ai"] {
        let hosted_key = format!("{vertex_root}/claude-sonnet-4");
        let mut litellm = HashMap::new();
        litellm.insert(
            hosted_key.clone(),
            ModelPricing {
                input_cost_per_token: Some(0.000003),
                output_cost_per_token: Some(0.000015),
                ..Default::default()
            },
        );
        litellm.insert(
            "anthropic/claude-sonnet-4".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.000004),
                output_cost_per_token: Some(0.000020),
                ..Default::default()
            },
        );
        let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

        let vertex = lookup
            .lookup_with_provider("claude-sonnet-4", Some(vertex_root))
            .unwrap_or_else(|| panic!("{vertex_root}-hinted claude-sonnet-4 must price"));
        assert_eq!(vertex.matched_key, hosted_key);

        let anthropic = lookup
            .lookup_with_provider("claude-sonnet-4", Some("anthropic"))
            .expect("anthropic-hinted claude-sonnet-4 must price");
        assert_eq!(anthropic.matched_key, "anthropic/claude-sonnet-4");
    }
}

/// The same explicit-root preference must survive source arbitration;
/// otherwise each dataset selects correctly and the later cross-source
/// first-party tier silently changes the winner back to Anthropic.
#[test]
fn direct_vertex_hint_outranks_cross_source_anthropic_alias() {
    for vertex_root in ["vertex", "vertex_ai"] {
        let hosted_key = format!("{vertex_root}/claude-sonnet-4");
        let mut litellm = HashMap::new();
        litellm.insert(
            hosted_key.clone(),
            ModelPricing {
                input_cost_per_token: Some(0.000003),
                output_cost_per_token: Some(0.000015),
                ..Default::default()
            },
        );
        let mut openrouter = HashMap::new();
        openrouter.insert(
            "anthropic/claude-sonnet-4".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.000004),
                output_cost_per_token: Some(0.000020),
                ..Default::default()
            },
        );
        let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());

        let vertex = lookup
            .lookup_with_provider("claude-sonnet-4", Some(vertex_root))
            .unwrap_or_else(|| panic!("{vertex_root}-hinted claude-sonnet-4 must price"));
        assert_eq!(vertex.matched_key, hosted_key);
        assert_eq!(vertex.source, "LiteLLM");

        let anthropic = lookup
            .lookup_with_provider("claude-sonnet-4", Some("anthropic"))
            .expect("anthropic-hinted claude-sonnet-4 must price");
        assert_eq!(anthropic.matched_key, "anthropic/claude-sonnet-4");
        assert_eq!(anthropic.source, "OpenRouter");
    }
}

/// `vertex` and `vertex_ai` share a provider tag for fallback reachability,
/// but are distinct billing endpoints. The literal root must win in either
/// direction even though the longer `vertex_ai` key is ordered first.
#[test]
fn vertex_endpoint_aliases_do_not_impersonate_each_others_own_root() {
    let mut litellm = HashMap::new();
    for key in ["vertex/claude-sonnet-4", "vertex_ai/claude-sonnet-4"] {
        litellm.insert(
            key.to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.000003),
                output_cost_per_token: Some(0.000015),
                ..Default::default()
            },
        );
    }
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    for hint in ["vertex", "vertex_ai"] {
        let result = lookup
            .lookup_with_provider("claude-sonnet-4", Some(hint))
            .unwrap_or_else(|| panic!("{hint}-hinted claude-sonnet-4 must price"));
        assert_eq!(result.matched_key, format!("{hint}/claude-sonnet-4"));
    }
}

#[test]
fn vertex_endpoint_literal_root_survives_cross_source_arbitration() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "vertex/claude-sonnet-4".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            ..Default::default()
        },
    );
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "vertex_ai/claude-sonnet-4".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000004),
            output_cost_per_token: Some(0.000020),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());

    for (hint, key, source) in [
        ("vertex", "vertex/claude-sonnet-4", "LiteLLM"),
        ("vertex_ai", "vertex_ai/claude-sonnet-4", "OpenRouter"),
    ] {
        let result = lookup
            .lookup_with_provider("claude-sonnet-4", Some(hint))
            .unwrap_or_else(|| panic!("{hint}-hinted claude-sonnet-4 must price"));
        assert_eq!(result.matched_key, key);
        assert_eq!(result.source, source);
    }
}

/// A literal provider root in Models.dev must participate in the same
/// arbitration as LiteLLM and OpenRouter instead of losing to their
/// alias-only row merely because Models.dev is normally the long-tail
/// fallback. Exercise both directions of the Anthropic/Vertex relation.
#[test]
fn models_dev_literal_root_outranks_cross_source_endpoint_alias() {
    for (hint, own_root, alias_root) in [
        ("vertex", "vertex", "anthropic"),
        ("vertex_ai", "vertex_ai", "anthropic"),
        ("anthropic", "anthropic", "vertex"),
        ("anthropic", "anthropic", "vertex_ai"),
    ] {
        let mut litellm = HashMap::new();
        litellm.insert(
            format!("{alias_root}/claude-sonnet-4"),
            ModelPricing {
                input_cost_per_token: Some(0.000003),
                output_cost_per_token: Some(0.000015),
                ..Default::default()
            },
        );
        let mut models_dev = HashMap::new();
        let own_key = format!("{own_root}/claude-sonnet-4");
        models_dev.insert(
            own_key.clone(),
            ModelPricing {
                input_cost_per_token: Some(0.000004),
                output_cost_per_token: Some(0.000020),
                ..Default::default()
            },
        );
        let lookup = PricingLookup::new_with_models_dev(
            litellm,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            models_dev,
        );

        let result = lookup
            .lookup_with_provider("claude-sonnet-4", Some(hint))
            .unwrap_or_else(|| panic!("{hint}-hinted claude-sonnet-4 must price"));
        assert_eq!(result.matched_key, own_key);
        assert_eq!(result.source, "Models.dev");
    }
}

#[test]
fn normalized_models_dev_literal_root_outranks_cross_source_endpoint_alias() {
    for (hint, own_root, alias_root) in [
        ("vertex", "vertex", "anthropic"),
        ("vertex_ai", "vertex_ai", "anthropic"),
        ("anthropic", "anthropic", "vertex"),
        ("anthropic", "anthropic", "vertex_ai"),
    ] {
        let mut openrouter = HashMap::new();
        openrouter.insert(
            format!("{alias_root}/claude-sonnet-4-6"),
            ModelPricing {
                input_cost_per_token: Some(0.000003),
                output_cost_per_token: Some(0.000015),
                ..Default::default()
            },
        );
        let mut models_dev = HashMap::new();
        let own_key = format!("{own_root}/claude-sonnet-4-6");
        models_dev.insert(
            own_key.clone(),
            ModelPricing {
                input_cost_per_token: Some(0.000004),
                output_cost_per_token: Some(0.000020),
                ..Default::default()
            },
        );
        let lookup = PricingLookup::new_with_models_dev(
            HashMap::new(),
            openrouter,
            HashMap::new(),
            HashMap::new(),
            models_dev,
        );

        let result = lookup
            .lookup_with_provider("claude-sonnet-4.6", Some(hint))
            .unwrap_or_else(|| panic!("normalized {hint}-hinted Claude must price"));
        assert_eq!(result.matched_key, own_key);
        assert_eq!(result.source, "Models.dev");
    }
}

/// A root globally classified as a reseller can still be the hinted
/// provider's own top-level row. Together's row must retain the root tier
/// over a longer host that merely nests the Together spelling.
#[test]
fn reseller_classification_does_not_hide_hinted_provider_own_root() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "together_ai/model-x".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000001),
            output_cost_per_token: Some(0.000002),
            ..Default::default()
        },
    );
    litellm.insert(
        "long-host/together/model-x".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000004),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let result = lookup
        .lookup_with_provider("model-x", Some("together"))
        .expect("together-hinted model-x must price");
    assert_eq!(result.matched_key, "together_ai/model-x");
}
