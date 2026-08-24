use crate::pricing::lookup::provider::RESELLER_PROVIDER_PREFIXES;
use crate::provider_identity;

const ORIGINAL_PROVIDER_PREFIXES: &[&str] = &[
    "x-ai/",
    "xai/",
    "anthropic/",
    "openai/",
    "google/",
    "meta-llama/",
    "mistralai/",
    "minimax/",
    "deepseek/",
    "z-ai/",
    "qwen/",
    "cohere/",
    "perplexity/",
    "moonshotai/",
];

/// Deterministic provider choice when multiple models.dev providers share a
/// model part: the canonical `anthropic/` namespace wins outright; otherwise
/// the shorter key is preferred (the historical winner of the insertion-order
/// race, keeping existing resolutions stable), with lexicographic order
/// breaking length ties so the result no longer depends on HashMap iteration
/// order.
// @keep: the shortest-key fallback is arbitrary and actively harmful; the
// original-provider preference in front of it is what makes this defensible.
/// Elect between two dataset keys that share a model part.
///
/// Preferring the ORIGINAL provider generalizes what used to be a hardcoded
/// `anthropic/` special case. The rule it encodes is the same one that
/// motivated that case: when several vendors publish a key ending in the same
/// model name, the vendor who made the model is the one whose rates describe
/// it — a reseller or aggregator row is at best a repackaging.
///
/// Length is the last resort and is a coin-flip, not a signal. It is what
/// elected `morph/auto` ($0.85/$1.55) over three $0.00 router rows for the
/// model part `auto` (#1062), i.e. the single worst-priced candidate purely
/// because its key was ten characters. Routing labels no longer reach here at
/// all, but the same hazard remains for any model part several vendors share,
/// so prefer adding the real vendor to ORIGINAL_PROVIDER_PREFIXES over
/// relying on the tie-break to land correctly.
pub(in crate::pricing::lookup) fn prefers_model_part_key(candidate: &str, existing: &str) -> bool {
    let candidate_lower = candidate.to_lowercase();
    let existing_lower = existing.to_lowercase();
    match (
        is_original_provider(&candidate_lower),
        is_original_provider(&existing_lower),
    ) {
        (true, false) => true,
        (false, true) => false,
        _ => (candidate_lower.len(), candidate_lower) < (existing_lower.len(), existing_lower),
    }
}

pub(in crate::pricing::lookup) fn is_original_provider(key: &str) -> bool {
    let lower = key.to_lowercase();
    ORIGINAL_PROVIDER_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Whether the dataset key's leading segment *is* the hinted vendor, rather
/// than a reseller that merely nests the vendor deeper in the key.
///
/// `poe/novita/kimi-k2.6` and `novita-ai/moonshotai/kimi-k2.6` both carry the
/// tag `novita`, but only the second is Novita's own row; the first is Poe
/// reselling it at $0.96/$4.04 per MTok against Novita's $0.80/$3.40.
pub(super) fn key_root_matches_hint(key: &str, hint_tags: &[String]) -> bool {
    let Some(root) = key.split('/').next() else {
        return false;
    };
    provider_identity::provider_tags(root)
        .iter()
        .any(|tag| hint_tags.iter().any(|hint| hint == tag))
}

/// Whether provider-tag folding makes the key root and hint match despite
/// naming different billing endpoints. The alias keeps fallback rows reachable,
/// but neither endpoint's root is the other endpoint's own top-level row.
pub(super) fn key_root_is_cross_provider_alias(key: &str, provider_id: &str) -> bool {
    let normalize_root = |value: &str| {
        value
            .trim()
            .trim_end_matches('/')
            .split('/')
            .next()
            .unwrap_or_default()
            .to_lowercase()
            .replace('-', "_")
    };
    let root = normalize_root(key);
    let hint = normalize_root(provider_id);

    let is_claude_endpoint = |value: &str| matches!(value, "anthropic" | "vertex" | "vertex_ai");
    root != hint && is_claude_endpoint(&root) && is_claude_endpoint(&hint)
}

pub(super) fn key_root_matches_provider_hint(key: &str, provider_id: &str) -> bool {
    let hint_tags = provider_identity::provider_tags(provider_id);
    key_root_matches_hint(key, &hint_tags) && !key_root_is_cross_provider_alias(key, provider_id)
}

pub(in crate::pricing::lookup) fn is_reseller_provider(key: &str) -> bool {
    // Provider-stage contract: scoped-path matching and result ranking share
    // one reseller-prefix list so the two stages classify the same keys.
    let lower = key.to_lowercase();
    RESELLER_PROVIDER_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}
