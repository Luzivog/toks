use crate::{model_alias, opencode_model_name};

/// Strip a CLIProxyAPI-style `(level)` reasoning-effort suffix from a model id.
///
/// Mirrors <https://help.router-for.me/configuration/thinking>: the proxy
/// strips the parentheses before routing, so for pricing lookups we treat the
/// suffix as cosmetic and resolve to the base model. Accepts the documented
/// levels case-insensitively because callers pass a lowercased id.
pub(crate) fn strip_parenthesized_reasoning_tier(model_id: &str) -> Option<&str> {
    let without_closing_paren = model_id.strip_suffix(')')?;
    let (base_model, tier) = without_closing_paren.rsplit_once('(')?;

    if base_model.is_empty() || base_model.trim() != base_model {
        return None;
    }
    if !matches!(
        tier,
        "minimal" | "low" | "medium" | "high" | "xhigh" | "auto" | "none"
    ) {
        return None;
    }
    Some(base_model)
}

/// Canonical model identity, without machine-local alias folding.
pub fn canonical_model_id(model_id: &str) -> String {
    normalize_syntactic(model_id)
}

/// Local display and grouping model name with configured aliases applied.
pub fn normalize_model_for_grouping(model_id: &str) -> String {
    model_alias::global().apply(normalize_syntactic(model_id))
}

/// Apply OpenCode's configured label when one exists, then use normal grouping.
pub fn model_name_for_grouping(client: &str, provider_id: &str, model_id: &str) -> String {
    let fallback = normalize_model_for_grouping(model_id);
    if client == "opencode" {
        opencode_model_name::global()
            .display_name(provider_id, model_id)
            .map(str::to_string)
            .unwrap_or(fallback)
    } else {
        fallback
    }
}

/// Structural-only model normalization shared by aliases and persisted ids.
pub(crate) fn normalize_syntactic(model_id: &str) -> String {
    let mut name = model_id.to_lowercase();

    if let Some(base_model) = strip_parenthesized_reasoning_tier(&name) {
        name = base_model.to_string();
    }
    if name.len() > 9 {
        let potential_date = &name[name.len() - 8..];
        if potential_date.chars().all(|c| c.is_ascii_digit())
            && name.as_bytes()[name.len() - 9] == b'-'
        {
            name = name[..name.len() - 9].to_string();
        }
    }

    if name.contains("claude") {
        let chars: Vec<char> = name.chars().collect();
        let mut result = String::with_capacity(name.len());
        for i in 0..chars.len() {
            if chars[i] == '.'
                && i > 0
                && i < chars.len() - 1
                && chars[i - 1].is_ascii_digit()
                && chars[i + 1].is_ascii_digit()
            {
                result.push('-');
            } else {
                result.push(chars[i]);
            }
        }
        name = result;
    }

    if let Some(canonical) = normalize_anthropic_prefixed_claude_model(&name) {
        name = canonical;
    }
    name
}

fn normalize_anthropic_prefixed_claude_model(model_id: &str) -> Option<String> {
    let rest = model_id.strip_prefix("anthropic/claude-")?;
    let mut parts = rest.split('-');
    let major = parts.next()?;
    let minor = parts.next()?;
    let family = parts.next()?;
    if parts.next().is_some() || !matches!(family, "opus" | "sonnet" | "haiku") {
        return None;
    }
    Some(format!("claude-{family}-{major}-{minor}"))
}
