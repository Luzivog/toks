use super::delimiters::contains_delimited_fragment;
use super::version::{
    contains_delimited_modern_major_minor, normalize_claude_family_bare_major,
    normalize_claude_family_minor,
};

pub(in crate::pricing::lookup) fn normalize_model_name(model_id: &str) -> Option<String> {
    let lower = model_id.to_lowercase();
    let family = claude_family(&lower)?;

    // Modern Claude line (major >= 4): explicit single-digit minor parsed
    // straight from the id, in either order (claude-sonnet-4-6, opus-4.8,
    // claude-4-6-sonnet). New minor releases need no code change.
    if let Some(model) = normalize_claude_family_minor(&lower) {
        return Some(model);
    }

    // Never degrade: a delimited `major(-|.)minor` version whose minor was
    // not recognized above (4-60, 4-0, 5-0, dated 4-20250514) must stay
    // unresolved rather than fall through to a coarser or older key.
    if contains_delimited_modern_major_minor(&lower) {
        return None;
    }

    // Bare modern major adjacent to the family token (claude-sonnet-5,
    // opus-5, 4-opus). Resolves only via an exact dataset hit downstream.
    if let Some(model) = normalize_claude_family_bare_major(&lower) {
        return Some(model);
    }

    // Catch-alls preserved from the hardcoded matcher: a delimited `4`
    // anywhere still maps opus/sonnet to the bare 4.0 key, and the legacy
    // 3.x line uses irregular naming (family after the version, dotted 3.5).
    match family {
        "opus" if contains_delimited_fragment(&lower, "4") => Some("claude-opus-4".into()),
        "sonnet" => {
            if contains_delimited_fragment(&lower, "4") {
                Some("claude-sonnet-4".into())
            } else if contains_delimited_fragment(&lower, "3.7")
                || contains_delimited_fragment(&lower, "3-7")
            {
                Some("claude-3-7-sonnet".into())
            } else if contains_delimited_fragment(&lower, "3.5")
                || contains_delimited_fragment(&lower, "3-5")
            {
                Some("claude-3.5-sonnet".into())
            } else {
                None
            }
        }
        "haiku" => {
            if contains_delimited_fragment(&lower, "3.5")
                || contains_delimited_fragment(&lower, "3-5")
            {
                Some("claude-3.5-haiku".into())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Family tokens of the modern Claude model line.
pub(super) const CLAUDE_FAMILY_TOKENS: &[&str] = &["opus", "sonnet", "haiku", "fable"];

/// The Claude family token contained in `lower`, if any.
pub(in crate::pricing::lookup) fn claude_family(lower: &str) -> Option<&'static str> {
    CLAUDE_FAMILY_TOKENS
        .iter()
        .copied()
        .find(|family| lower.contains(family))
}
