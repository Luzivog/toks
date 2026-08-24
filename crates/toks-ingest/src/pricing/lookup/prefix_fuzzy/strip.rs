use super::super::normalize::{contains_delimited_fragment, contains_delimited_major_minor};
use super::super::LookupResult;

/// Minimum length for a model name candidate after prefix/suffix stripping.
/// Prevents false positives like "pro" or "flash" being matched alone.
const MIN_MODEL_NAME_LEN: usize = 2;

/// Maximum number of leading segments that can be treated as a routing prefix.
/// Limits how aggressively we strip (e.g., "a-b-claude-3" strips at most "a-b-").
const MAX_PREFIX_STRIP_SEGMENTS: usize = 2;

/// Maximum number of trailing segments that can be treated as a routing suffix.
/// Handles tier suffixes (-high, -low) and variant suffixes (-thinking, -codex, -codex-max-xhigh).
const MAX_SUFFIX_STRIP_SEGMENTS: usize = 4;

/// Attempts to find a model by progressively stripping trailing segments.
/// Handles arbitrary suffixes (e.g., "claude-sonnet-4-5-thinking" → "claude-sonnet-4-5").
/// This replaces the hardcoded TIER_SUFFIXES and FALLBACK_SUFFIXES approach.
pub(in crate::pricing::lookup) fn try_strip_unknown_suffix<F>(
    model_id: &str,
    do_lookup: F,
) -> Option<LookupResult>
where
    F: Fn(&str) -> Option<LookupResult>,
{
    if has_unrecognized_claude_four_minor(model_id) {
        return None;
    }

    let parts: Vec<&str> = model_id.split('-').collect();

    if parts.len() < 2 {
        return None;
    }

    let max_strip = std::cmp::min(parts.len() - 1, MAX_SUFFIX_STRIP_SEGMENTS);

    for strip in 1..=max_strip {
        let candidate: String = parts[..parts.len() - strip].join("-");

        if candidate.len() >= MIN_MODEL_NAME_LEN {
            if strips_claude_numeric_minor(&candidate, parts[parts.len() - strip]) {
                continue;
            }

            if let Some(result) = do_lookup(&candidate) {
                return Some(result);
            }
        }
    }

    None
}

fn strips_claude_numeric_minor(candidate: &str, first_stripped_segment: &str) -> bool {
    if !is_version_segment(first_stripped_segment) {
        return false;
    }
    let claude_branded = candidate.contains("claude")
        || candidate.contains("opus")
        || candidate.contains("sonnet")
        || candidate.contains("haiku");
    if !claude_branded {
        return false;
    }
    // Refuse to strip a version segment when it would either peel a minor off
    // a still-versioned claude-4 candidate (claude-sonnet-4-5 -> claude-sonnet-4)
    // or erode the id's only version, leaving a bare brand token
    // (claude-2.1 -> claude). Both candidates would resolve to a different
    // model's price. Dated forms (claude-3-5-sonnet-20241022) keep stripping:
    // their candidate retains a version, so neither arm fires.
    contains_delimited_fragment(candidate, "4") || !candidate.bytes().any(|b| b.is_ascii_digit())
}

/// True for a bare version segment produced by splitting an id on `-`:
/// digits with at most one interior dot (`4`, `6`, `2.1`, `20241022`).
fn is_version_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() || !bytes[bytes.len() - 1].is_ascii_digit() {
        return false;
    }
    let mut seen_dot = false;
    for &byte in bytes {
        match byte {
            b'0'..=b'9' => {}
            b'.' if !seen_dot => seen_dot = true,
            _ => return false,
        }
    }
    true
}

fn has_unrecognized_claude_four_minor(model_id: &str) -> bool {
    (model_id.contains("claude")
        || model_id.contains("opus")
        || model_id.contains("sonnet")
        || model_id.contains("haiku"))
        && contains_delimited_major_minor(model_id, '4')
        && !contains_delimited_fragment(model_id, "4.5")
        && !contains_delimited_fragment(model_id, "4-5")
        && !contains_delimited_fragment(model_id, "4.6")
        && !contains_delimited_fragment(model_id, "4-6")
        && !contains_delimited_fragment(model_id, "4.7")
        && !contains_delimited_fragment(model_id, "4-7")
}

/// Attempts to find a model by progressively stripping leading segments.
/// Handles arbitrary routing prefixes (e.g., "myplugin-claude-3.5-sonnet" → "claude-3.5-sonnet").
/// This replaces the hardcoded STRIPPED_PREFIXES approach.
pub(in crate::pricing::lookup) fn try_strip_unknown_prefix<F>(
    model_id: &str,
    do_lookup: F,
) -> Option<LookupResult>
where
    F: Fn(&str) -> Option<LookupResult>,
{
    let parts: Vec<&str> = model_id.split('-').collect();

    if parts.len() < 2 {
        return None;
    }

    let max_skip = std::cmp::min(parts.len() - 1, MAX_PREFIX_STRIP_SEGMENTS);

    for skip in 1..=max_skip {
        let candidate: String = parts[skip..].join("-");

        if candidate.len() >= MIN_MODEL_NAME_LEN {
            // Try candidate directly
            if let Some(result) = do_lookup(&candidate) {
                return Some(result);
            }

            // Try candidate with suffix stripping
            if let Some(result) = try_strip_unknown_suffix(&candidate, &do_lookup) {
                return Some(result);
            }
        }
    }

    None
}
