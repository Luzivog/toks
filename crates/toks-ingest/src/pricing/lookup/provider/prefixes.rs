use super::PROVIDER_PREFIXES;

pub(in crate::pricing::lookup) fn strip_known_provider_prefix(model_id: &str) -> Option<&str> {
    for prefix in PROVIDER_PREFIXES {
        if let Some(stripped) = model_id.strip_prefix(prefix) {
            if !stripped.is_empty() {
                return Some(stripped);
            }
        }
    }
    None
}

/// Generic routing-prefix fallback for ids whose leading segment is not one
/// of the curated `PROVIDER_PREFIXES` (e.g. `cx/gpt-5.5` routed through an
/// `omniroute` proxy, or any other CLI/router-assigned alias). Returns the
/// terminal path segment — the part after the last `/` — when the id
/// actually contains a `/`, so `cx/gpt-5.5` resolves to `gpt-5.5`.
///
/// This is intentionally unconditional (unlike `strip_known_provider_prefix`,
/// which only recognizes canonical LLM provider names): the caller only
/// invokes it as a fallback AFTER the exact/direct lookup on the full id has
/// already failed, so dataset keys that legitimately keep their prefix (e.g.
/// `anthropic/claude-fable-5`) are resolved by their own exact key first and
/// never reach this fallback.
pub(in crate::pricing::lookup) fn strip_generic_provider_prefix(model_id: &str) -> Option<&str> {
    let terminal = model_id.rsplit('/').next()?;
    if terminal.is_empty() || terminal == model_id {
        return None;
    }
    Some(terminal)
}
