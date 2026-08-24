use super::super::*;

/// The shortest-key tie-break is a coin flip. Preferring the original
/// provider generalizes the `anthropic/` special case it replaced, so a
/// reseller no longer wins on key length alone.
#[test]
fn model_part_tie_break_prefers_the_original_provider_over_a_shorter_key() {
    assert!(super::prefers_model_part_key(
        "openai/some-model",
        "xy/some-model"
    ));
    assert!(!super::prefers_model_part_key(
        "xy/some-model",
        "openai/some-model"
    ));
    // Neither is an original provider: length still decides.
    assert!(super::prefers_model_part_key(
        "ab/some-model",
        "abcd/some-model"
    ));
}

/// Folding `deepseek-ai` into `deepseek` widens the provider-hint
/// candidate pool, and `deepseek` is exactly the hint
/// `inferred_provider_from_model` synthesizes for every model named
/// `deepseek-*` whose client reports no provider. Both rows below then
/// match the hint, they disagree by 16x on output, and nothing else in
/// `select_best_match` tells them apart — the winner would fall out of key
/// ordering, which is length-descending over a HashMap's key iteration and
/// so not stable between processes for equal-length keys. The row spelling
/// the vendor the way the hint does has to win.
#[test]
fn vendor_spelling_fold_does_not_move_pricing_onto_another_reseller() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "novita/deepseek/deepseek-r1-distill-qwen-32b".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000003),
            output_cost_per_token: Some(0.0000003),
            ..Default::default()
        },
    );
    litellm.insert(
        "cloudflare/@cf/deepseek-ai/deepseek-r1-distill-qwen-32b".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000000497),
            output_cost_per_token: Some(0.000004881),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let result = lookup
        .lookup_with_provider("deepseek-r1-distill-qwen-32b", Some("deepseek"))
        .expect("deepseek-hinted distill must price");
    assert_eq!(
        result.matched_key, "novita/deepseek/deepseek-r1-distill-qwen-32b",
        "a `deepseek` hint must not cross onto the `deepseek-ai`-spelled reseller row"
    );
    assert_eq!(result.pricing.output_cost_per_token, Some(0.0000003));
}

/// The other direction of the same fold, which the spelling preference must
/// not break: a `deepseek-ai` hint exists so it can reach rows spelled
/// `deepseek`, and DeepSeek's own first-party row is the whole point. A
/// reseller row that happens to spell the vendor the hint's way must not
/// displace it.
#[test]
fn vendor_spelling_preference_never_displaces_the_first_party_row() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "deepseek/deepseek-v3".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.00000027),
            output_cost_per_token: Some(0.0000011),
            ..Default::default()
        },
    );
    litellm.insert(
        "hyperbolic/deepseek-ai/DeepSeek-V3".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000002),
            output_cost_per_token: Some(0.0000002),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    for hint in ["deepseek-ai", "deepseek_ai", "DeepSeek-AI", "deepseek"] {
        let result = lookup
            .lookup_with_provider("deepseek-v3", Some(hint))
            .unwrap_or_else(|| panic!("{hint}-hinted deepseek-v3 must price"));
        assert_eq!(
            result.matched_key, "deepseek/deepseek-v3",
            "{hint} must still reach DeepSeek's own row"
        );
    }
}

/// The spelling preference is a tiebreak among rows that merely nest the
/// vendor, so it must yield to the hinted provider's own top-level row.
/// `poe/novita/kimi-k2.6` spells `novita` only because Poe is reselling
/// Novita's endpoint, and it charges $0.96/$4.04 per MTok against Novita's
/// own $0.80/$3.40.
#[test]
fn hinted_provider_own_row_outranks_a_nested_spelling_match() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "novita-ai/moonshotai/kimi-k2.6".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000008),
            output_cost_per_token: Some(0.0000034),
            ..Default::default()
        },
    );
    litellm.insert(
        "poe/novita/kimi-k2.6".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.00000096),
            output_cost_per_token: Some(0.00000404),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let result = lookup
        .lookup_with_provider("kimi-k2.6", Some("novita"))
        .expect("novita-hinted kimi-k2.6 must price");
    assert_eq!(
        result.matched_key, "novita-ai/moonshotai/kimi-k2.6",
        "Novita's own row must win over Poe reselling it"
    );
}

/// Kimi, Warp, Kiro, Codebuff and Tencent Buddy report the literal string
/// `unknown` when they cannot name a provider, and `normalize_provider_hint`
/// drops it, so the unhinted path is reached in production. It has no
/// vendor spelling to prefer and must resolve exactly as a missing hint
/// does.
#[test]
fn unhinted_lookup_is_unchanged_by_the_vendor_spelling_preference() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "novita/deepseek/deepseek-r1-distill-qwen-32b".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000003),
            output_cost_per_token: Some(0.0000003),
            ..Default::default()
        },
    );
    litellm.insert(
        "cloudflare/@cf/deepseek-ai/deepseek-r1-distill-qwen-32b".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000000497),
            output_cost_per_token: Some(0.000004881),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let bare = lookup.lookup_with_provider("deepseek-r1-distill-qwen-32b", None);
    for hint in [Some("unknown"), Some("UNKNOWN"), Some(""), Some("  ")] {
        let hinted = lookup.lookup_with_provider("deepseek-r1-distill-qwen-32b", hint);
        assert_eq!(
            hinted.map(|r| r.matched_key),
            bare.as_ref().map(|r| r.matched_key.clone()),
            "{hint:?} is dropped by normalize_provider_hint and must match the unhinted result"
        );
    }
}

/// `key_root_matches_hint` recognises the hinted vendor's own top-level
/// row, and that row has to be *selected*, not merely used to switch the
/// spelling preference off. Z.ai publishes `zai/glm-4.6` at $0.60/$2.20 per
/// MTok and Vercel's gateway resells it at $0.45/$1.80 under
/// `vercel_ai_gateway/zai/glm-4.6`; neither key is in
/// `ORIGINAL_PROVIDER_PREFIXES` (Z.ai's first-party spelling there is
/// `z-ai/`) nor in `RESELLER_PROVIDER_PREFIXES`, and candidates are ordered
/// longest key first, so a `zai` hint must not be billed at the gateway's
/// sheet just because its key is longer.
#[test]
fn hinted_vendor_own_row_wins_over_a_longer_row_that_only_nests_the_vendor() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "zai/glm-4.6".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.0000006),
            output_cost_per_token: Some(0.0000022),
            ..Default::default()
        },
    );
    litellm.insert(
        "vercel_ai_gateway/zai/glm-4.6".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.00000045),
            output_cost_per_token: Some(0.0000018),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let result = lookup
        .lookup_with_provider("glm-4.6", Some("zai"))
        .expect("zai-hinted glm-4.6 must price");
    assert_eq!(
        result.matched_key, "zai/glm-4.6",
        "Z.ai's own row must win over a gateway that nests `zai` in a longer key"
    );
    assert_eq!(result.pricing.output_cost_per_token, Some(0.0000022));
}

/// The spelling preference exists to keep a hinted vendor on the row that
/// spells the vendor its way, so it must not throw that row away for
/// starting with a reseller prefix. Before the fold, a `deepseek-ai` hint
/// matched only `together_ai/deepseek-ai/DeepSeek-R1` ($3.00/$7.00 per
/// MTok); folding `deepseek-ai` into `deepseek` pulled
/// `vercel_ai_gateway/deepseek/deepseek-r1` ($0.55/$2.19) into the same
/// pool, and it wins on key length alone. That is the fold moving usage
/// between two resellers, which is precisely what the preference is for.
#[test]
fn exact_vendor_spelling_wins_even_when_that_row_is_a_reseller() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "together_ai/deepseek-ai/DeepSeek-R1".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000007),
            ..Default::default()
        },
    );
    litellm.insert(
        "vercel_ai_gateway/deepseek/deepseek-r1".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.00000055),
            output_cost_per_token: Some(0.00000219),
            ..Default::default()
        },
    );
    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let result = lookup
        .lookup_with_provider("deepseek-r1", Some("deepseek-ai"))
        .expect("deepseek-ai-hinted deepseek-r1 must price");
    assert_eq!(
        result.matched_key, "together_ai/deepseek-ai/DeepSeek-R1",
        "the row spelling the vendor the hint's way must win even though it is a reseller"
    );
    assert_eq!(result.pricing.output_cost_per_token, Some(0.000007));
}

/// Vertex canonicalizes to Anthropic so an Anthropic hint can find Vertex's
/// hosted Claude rows. That alias must not make the hosting platform's root
/// look like Anthropic's own top-level row and outrank an exact-spelling row.
#[test]
fn reseller_alias_root_does_not_outrank_exact_vendor_spelling() {
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
            "host/anthropic/claude-sonnet-4".to_string(),
            ModelPricing {
                input_cost_per_token: Some(0.000004),
                output_cost_per_token: Some(0.000020),
                ..Default::default()
            },
        );
        let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

        let anthropic = lookup
            .lookup_with_provider("claude-sonnet-4", Some("anthropic"))
            .expect("anthropic-hinted claude-sonnet-4 must price");
        assert_eq!(
            anthropic.matched_key, "host/anthropic/claude-sonnet-4",
            "{vertex_root} must not impersonate Anthropic's own root"
        );

        let vertex = lookup
            .lookup_with_provider("claude-sonnet-4", Some(vertex_root))
            .unwrap_or_else(|| panic!("{vertex_root}-hinted claude-sonnet-4 must price"));
        assert_eq!(
            vertex.matched_key, hosted_key,
            "a Vertex hint must still select Vertex's hosted row"
        );
    }
}
