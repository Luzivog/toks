use super::super::*;

#[test]
fn test_provider_hint_matches_nested_google_segment_during_fuzzy_lookup() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openrouter/google/gemini-3-pro-preview".into(),
        ModelPricing {
            input_cost_per_token: Some(1.0),
            ..Default::default()
        },
    );
    litellm.insert(
        "vertex_ai/gemini-3-pro-preview-max".into(),
        ModelPricing {
            input_cost_per_token: Some(2.0),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup
        .lookup_with_provider("gemini-3-pro", Some("google"))
        .unwrap();
    assert_eq!(result.matched_key, "openrouter/google/gemini-3-pro-preview");
    assert_eq!(result.source, "LiteLLM");
}

#[test]
fn test_cross_source_fuzzy_provider_hint_wins_over_original_provider_fallback() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "fireworks_ai/deepseek-v3-0324".into(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            ..Default::default()
        },
    );

    let mut openrouter = HashMap::new();
    openrouter.insert(
        "deepseek/deepseek-v3-0324".into(),
        ModelPricing {
            input_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, openrouter, HashMap::new());
    let result = lookup
        .lookup_with_provider("deepseek-v3", Some("fireworks"))
        .unwrap();
    assert_eq!(result.matched_key, "fireworks_ai/deepseek-v3-0324");
    assert_eq!(result.source, "LiteLLM");
}

/// Regression (post-#634 catalog audit, bug 2): `claude-opus-4-6-fast`
/// must hit the canonical OpenRouter `anthropic/claude-opus-4.6-fast`
/// key ($30/$150) via separator normalization, not Models.dev's reseller
/// `venice/claude-opus-4-6-fast` markup ($36/$180). Previously the
/// models.dev model-part pass ran before the version-normalized
/// OpenRouter exact pass in `lookup_auto`.
#[test]
fn canonical_fast_price_beats_reseller_markup() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4.6-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(30e-6),
            output_cost_per_token: Some(150e-6),
            ..Default::default()
        },
    );
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "venice/claude-opus-4-6-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(36e-6),
            output_cost_per_token: Some(180e-6),
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

    let result = lookup.lookup("claude-opus-4-6-fast").unwrap();
    assert_eq!(result.matched_key, "anthropic/claude-opus-4.6-fast");
    assert_eq!(result.pricing.input_cost_per_token, Some(30e-6));
}

/// Regression (#707 review): a provider hint pins the lookup to that
/// provider's catalog. The canonical-source reorder asserted by
/// `canonical_fast_price_beats_reseller_markup` only applies to unhinted
/// lookups; with `provider_id = Some("venice")` the provider-scoped
/// models.dev pass must win over OpenRouter's unscoped `anthropic/...`
/// row, so provider-aware callers get the hinted provider's price.
#[test]
fn provider_hint_keeps_models_dev_provider_key_over_unscoped_canonical() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4.6-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(30e-6),
            output_cost_per_token: Some(150e-6),
            ..Default::default()
        },
    );
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "venice/claude-opus-4-6-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(36e-6),
            output_cost_per_token: Some(180e-6),
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

    let hinted = lookup
        .lookup_with_provider("claude-opus-4-6-fast", Some("venice"))
        .unwrap();
    assert_eq!(hinted.matched_key, "venice/claude-opus-4-6-fast");
    assert_eq!(hinted.pricing.input_cost_per_token, Some(36e-6));

    // Unhinted lookups keep the canonical resolution.
    let unhinted = lookup.lookup("claude-opus-4-6-fast").unwrap();
    assert_eq!(unhinted.matched_key, "anthropic/claude-opus-4.6-fast");
    assert_eq!(unhinted.pricing.input_cost_per_token, Some(30e-6));
}

/// Regression (#1004 follow-up): a reseller provider hint must select the
/// reseller-scoped models.dev row instead of a direct upstream catalog row
/// with the same terminal model id.
#[test]
fn orcarouter_hint_selects_orcarouter_models_dev_row() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "openai/gpt-5.5".to_string(),
        ModelPricing {
            input_cost_per_token: Some(5e-6),
            output_cost_per_token: Some(30e-6),
            ..Default::default()
        },
    );
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "orcarouter/openai/gpt-5.5".to_string(),
        ModelPricing {
            input_cost_per_token: Some(8e-6),
            output_cost_per_token: Some(48e-6),
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
        .lookup_with_provider("gpt-5.5", Some("orcarouter"))
        .unwrap();
    assert_eq!(result.source, "Models.dev");
    assert_eq!(result.matched_key, "orcarouter/openai/gpt-5.5");
    assert_eq!(result.pricing.input_cost_per_token, Some(8e-6));
}

/// Regression (#707 review, cubic follow-up): the provider-hint pin must
/// also beat the unscoped OpenRouter MODEL-PART fallback, not just the
/// separator-normalized passes. When the hinted provider's models.dev key
/// shares the dotted model-part spelling that OpenRouter already indexes
/// (here both `claude-opus-4.6-fast`), an unscoped model-part match would
/// otherwise return `anthropic/...` before the provider-scoped pass ran.
#[test]
fn provider_hint_beats_unscoped_openrouter_model_part_for_dotted_id() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "anthropic/claude-opus-4.6-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(30e-6),
            output_cost_per_token: Some(150e-6),
            ..Default::default()
        },
    );
    let mut models_dev = HashMap::new();
    // Hinted provider's key uses the SAME dotted spelling OpenRouter
    // indexes as a model-part — this is what makes the unscoped model-part
    // pass fire first without the fix.
    models_dev.insert(
        "venice/claude-opus-4.6-fast".to_string(),
        ModelPricing {
            input_cost_per_token: Some(36e-6),
            output_cost_per_token: Some(180e-6),
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

    // Hinted dotted lookup must pin to venice, not the canonical OpenRouter
    // model-part it also matches.
    let hinted = lookup
        .lookup_with_provider("claude-opus-4.6-fast", Some("venice"))
        .unwrap();
    assert_eq!(hinted.matched_key, "venice/claude-opus-4.6-fast");
    assert_eq!(hinted.pricing.input_cost_per_token, Some(36e-6));

    // Unhinted dotted lookup keeps the canonical OpenRouter resolution.
    let unhinted = lookup.lookup("claude-opus-4.6-fast").unwrap();
    assert_eq!(unhinted.matched_key, "anthropic/claude-opus-4.6-fast");
    assert_eq!(unhinted.pricing.input_cost_per_token, Some(30e-6));

    // A hint for a provider with no matching key must still fall through to
    // the canonical resolution rather than returning None.
    let no_match = lookup
        .lookup_with_provider("claude-opus-4.6-fast", Some("groq"))
        .unwrap();
    assert_eq!(no_match.matched_key, "anthropic/claude-opus-4.6-fast");
    assert_eq!(no_match.pricing.input_cost_per_token, Some(30e-6));
}

/// Regression (#707 review): the anthropic-first preference in the
/// models.dev model-part index must only choose among priced keys. An
/// unpriced (all-None) `anthropic/<model>` row must not shadow a priced
/// reseller row, which would bill the model at zero cost.
#[test]
fn unpriced_anthropic_models_dev_key_does_not_shadow_priced_reseller() {
    let mut models_dev = HashMap::new();
    models_dev.insert("anthropic/model-x".to_string(), ModelPricing::default());
    models_dev.insert(
        "reseller/model-x".to_string(),
        ModelPricing {
            input_cost_per_token: Some(36e-6),
            output_cost_per_token: Some(180e-6),
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

    let result = lookup.lookup("model-x").unwrap();
    assert_eq!(result.matched_key, "reseller/model-x");
    assert_eq!(result.pricing.input_cost_per_token, Some(36e-6));
}

/// After the lookup_auto reorder, models.dev must remain the long-tail
/// fallback for ids no canonical source knows.
#[test]
fn models_dev_still_covers_long_tail_after_reorder() {
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "someprovider/exotic-model-9".to_string(),
        ModelPricing {
            input_cost_per_token: Some(2e-6),
            output_cost_per_token: Some(6e-6),
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

    let result = lookup.lookup("exotic-model-9").unwrap();
    assert_eq!(result.matched_key, "someprovider/exotic-model-9");
    assert_eq!(result.pricing.input_cost_per_token, Some(2e-6));
}

/// Regression (post-#634 catalog audit, bug 2b): when multiple models.dev
/// providers share a model part, the winner must be deterministic and
/// prefer the canonical `anthropic/` namespace. Previously the winner
/// depended on HashMap iteration order (with real data `302ai/` beat
/// `anthropic/` for claude-3-5-haiku-20241022 because shorter keys were
/// inserted last).
#[test]
fn models_dev_provider_choice_is_deterministic_and_prefers_anthropic() {
    let price = ModelPricing {
        input_cost_per_token: Some(0.8e-6),
        output_cost_per_token: Some(4e-6),
        ..Default::default()
    };
    // Adversarial insertion order: the non-canonical provider first.
    let mut models_dev = HashMap::new();
    models_dev.insert("302ai/claude-3-5-haiku-20241022".to_string(), price.clone());
    models_dev.insert(
        "anthropic/claude-3-5-haiku-20241022".to_string(),
        price.clone(),
    );
    let lookup = PricingLookup::new_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        models_dev,
    );

    let result = lookup.lookup("claude-3-5-haiku-20241022").unwrap();
    assert_eq!(result.matched_key, "anthropic/claude-3-5-haiku-20241022");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.8e-6));
}
