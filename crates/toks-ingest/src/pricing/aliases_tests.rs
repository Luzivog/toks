use super::resolve_alias;
use std::collections::HashMap;

#[test]
fn resolves_antigravity_placeholders() {
    let cases = [
        ("MODEL_PLACEHOLDER_M26", "claude-opus-4-6"),
        ("model_placeholder_m37", "gemini-3.1-pro"),
        ("model_placeholder_m16", "gemini-3.1-pro"),
        ("model_placeholder_m18", "gemini-3-flash-preview"),
        ("MODEL_PLACEHOLDER_M84", "gemini-3-flash-preview"),
        ("model_placeholder_m132", "gemini-3.5-flash-high"),
        ("model_placeholder_m133", "gemini-3.5-flash-high"),
        ("model_placeholder_m187", "gemini-3.5-flash-extra-low"),
        ("model_placeholder_m20", "gemini-3.5-flash-medium"),
        ("gemini-pro-default", "gemini-3.1-pro"),
        ("gemini-pro-agent", "gemini-3.1-pro"),
        ("gemini-3-flash-agent", "gemini-3.5-flash-high"),
        ("gemini-3-flash-b", "gemini-3.5-flash-high"),
        ("gemini-3.5-flash-low", "gemini-3.5-flash-medium"),
        ("MODEL_OPENAI_GPT_OSS_120B_MEDIUM", "gpt-oss-120b-medium"),
        ("gemini-3-flash-c", "gemini-3-flash-preview"),
        ("gemini-3-flash-a", "gemini-3.5-flash-high"),
        ("claude-opus-4.6-thinking", "claude-opus-4-6"),
        ("anthropic/claude-4-5-haiku", "claude-haiku-4-5"),
        ("anthropic/claude-4-6-sonnet", "claude-sonnet-4-6"),
    ];

    for (raw, expected) in cases {
        assert_eq!(resolve_alias(raw), Some(expected), "raw model: {raw}");
    }
}

#[test]
fn resolves_kimi_k2p6_aliases_without_regressing_k2p5() {
    assert_eq!(resolve_alias("k2p6"), Some("kimi-k2.6"));
    assert_eq!(resolve_alias("k2-p6"), Some("kimi-k2.6"));
    assert_eq!(resolve_alias("kimi-k2p6"), Some("kimi-k2.6"));
    assert_eq!(resolve_alias("KIMI-K2P6"), Some("kimi-k2.6"));

    assert_eq!(resolve_alias("k2p5"), Some("kimi-k2-thinking"));
    assert_eq!(resolve_alias("k2-p5"), Some("kimi-k2-thinking"));
}

#[test]
fn resolves_kimi_coding_plan_ids_to_underlying_models() {
    // kimi-code writes `kimi-code/<id>`; the parser strips the prefix, so
    // pricing sees the bare id. Without these, models.dev matches them under
    // its `kimi-for-coding/*` subscription namespace at $0.00.
    assert_eq!(
        resolve_alias("kimi-for-coding-highspeed"),
        Some("kimi-k2.7-code-highspeed")
    );
    assert_eq!(resolve_alias("k3"), Some("kimi-k3"));
}

#[test]
fn resolves_grok_composer_aliases_to_cursor_composer_prices() {
    assert_eq!(resolve_alias("grok-composer-2.5"), Some("composer-2.5"));
    assert_eq!(
        resolve_alias("GROK-COMPOSER-2.5-FAST"),
        Some("composer-2.5-fast")
    );
}

#[test]
fn m187_and_m20_resolve_to_distinct_tiers_but_both_still_price() {
    // M187 (true Low tier, machine id `gemini-3.5-flash-extra-low`) and
    // M20/raw CLI `gemini-3.5-flash-low` (actually the Medium tier) must
    // NOT collapse to the same canonical alias target — that would
    // silently merge two different-priced tiers into one cost bucket.
    // Verified against the pinned Antigravity Context Window Monitor SHA
    // (models.ts@603e3ea): M187's own `activeModelSpecs` entry has
    // `modelId: 'gemini-3.5-flash-extra-low'`, distinct from M20's
    // `modelId: 'gemini-3.5-flash-low'`.
    let m187_canonical = resolve_alias("model_placeholder_m187").unwrap();
    let m20_canonical = resolve_alias("model_placeholder_m20").unwrap();
    let cli_low_canonical = resolve_alias("gemini-3.5-flash-low").unwrap();

    assert_eq!(m187_canonical, "gemini-3.5-flash-extra-low");
    assert_eq!(m20_canonical, "gemini-3.5-flash-medium");
    assert_ne!(
        m187_canonical, m20_canonical,
        "M187 (Low) and M20 (Medium) must not resolve to the same tier"
    );
    // The raw CLI wire string tracks M20 (Medium), not M187 (Low).
    assert_eq!(cli_low_canonical, m20_canonical);

    // Both tiers must still reach a priced catalog entry: the pricing
    // dataset only carries one generic `google/gemini-3.5-flash` entry,
    // and the lookup's suffix-stripping normalization must land both the
    // `-extra-low` and `-medium` canonical ids on it.
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "google/gemini-3.5-flash".to_string(),
        super::super::litellm::ModelPricing {
            input_cost_per_token: Some(0.0000015),
            output_cost_per_token: Some(0.000009),
            ..Default::default()
        },
    );
    let lookup = super::super::lookup::PricingLookup::new_with_models_dev(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        models_dev,
    );

    let m187_result = lookup
        .lookup(m187_canonical)
        .expect("M187 target must still price via lookup normalization");
    let m20_result = lookup
        .lookup(m20_canonical)
        .expect("M20 target must still price via lookup normalization");

    assert_eq!(m187_result.matched_key, "google/gemini-3.5-flash");
    assert_eq!(m20_result.matched_key, "google/gemini-3.5-flash");
}

/// Regression: Ollama Cloud and other routers report MiniMax M3 as the
/// bare lowercase id `minimax-m3`, which matches no dataset key exactly.
/// Unhinted, it fell through to the model-part fallback and could elect any
/// third-party row publishing that model part — including the 0.0/0.0 rows
/// models.dev carries — instead of the first-party `minimax/MiniMax-M3`
/// key (#935).
#[test]
fn resolves_minimax_m3_bare_and_case_variants() {
    // resolve_alias is case-insensitive, since clients report mixed casing.
    assert_eq!(
        super::resolve_alias("minimax-m3"),
        Some("minimax/MiniMax-M3")
    );
    assert_eq!(
        super::resolve_alias("MiniMax-M3"),
        Some("minimax/MiniMax-M3")
    );
    assert_eq!(
        super::resolve_alias("MINIMAX-M3"),
        Some("minimax/MiniMax-M3")
    );
    // The qualified id already resolves via exact match; aliasing it too
    // would be harmless, but the bare form is the reported gap.
    assert_eq!(super::resolve_alias("minimax/minimax-m3"), None);
}

/// Regression: Anthropic's "-0" suffix is a documented moving alias for the
/// latest snapshot of a model line, and GitHub Copilot reports 4.1 without
/// the separator. Neither form resolved, so real first-party usage was
/// excluded from submission as unpriced — 41M tokens of claude-opus-4-0 in
/// one reported case.
#[test]
fn anthropic_moving_aliases_and_copilot_spelling_resolve() {
    assert_eq!(
        super::resolve_alias("claude-opus-4-0"),
        Some("claude-opus-4")
    );
    assert_eq!(
        super::resolve_alias("claude-sonnet-4-0"),
        Some("claude-sonnet-4")
    );
    assert_eq!(
        super::resolve_alias("claude-opus-41"),
        Some("claude-opus-4-1")
    );
    // Case-insensitive, since clients report mixed casing.
    assert_eq!(
        super::resolve_alias("Claude-Opus-4-0"),
        Some("claude-opus-4")
    );

    // Deliberately absent: `claude-sonnet-4-1` resolves cross-vendor to
    // `databricks/databricks-claude-sonnet-4-1` today (#1062), so aliasing
    // the Copilot spelling onto it would route usage to the wrong rates.
    assert_eq!(super::resolve_alias("claude-sonnet-41"), None);
}
