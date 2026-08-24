use super::claude::CLAUDE_FAMILY_TOKENS;
use super::delimiters::contains_delimited_major_minor;

/// Modern Claude majors are single digits >= 4. The 3.x line uses irregular
/// naming and is matched explicitly by the legacy branches.
fn is_modern_claude_major(value: &str) -> bool {
    value.len() == 1 && value.as_bytes()[0].is_ascii_digit() && value.as_bytes()[0] >= b'4'
}

/// Canonical `claude-{family}-{major}-{minor}` key parsed from an id carrying
/// an explicit single-digit minor for a modern major (>= 4), in either
/// `family-major-minor` (claude-sonnet-4-6, opus-4.8) or reversed
/// `major-minor-family` (claude-4-6-sonnet, 4-8-opus) order. Generalization
/// of the former opus-only `normalize_claude_opus_4_minor` across families.
pub(super) fn normalize_claude_family_minor(lower: &str) -> Option<String> {
    let parts: Vec<&str> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect();

    for window in parts.windows(3) {
        if CLAUDE_FAMILY_TOKENS.contains(&window[0])
            && is_modern_claude_major(window[1])
            && is_single_digit_minor(window[2])
        {
            return Some(format!("claude-{}-{}-{}", window[0], window[1], window[2]));
        }
        if is_modern_claude_major(window[0])
            && is_single_digit_minor(window[1])
            && CLAUDE_FAMILY_TOKENS.contains(&window[2])
        {
            return Some(format!("claude-{}-{}-{}", window[2], window[0], window[1]));
        }
    }

    None
}

/// Canonical `claude-{family}-{major}` key for an id naming a modern major
/// (>= 4) without a minor (claude-sonnet-5, opus-5, 4-opus). The major must
/// be adjacent to the family token; in forward order it must not be followed
/// by another digit run (dated `4-20250514` shapes are version-like, not
/// bare), and in reversed order it must not itself be the minor of a
/// preceding legacy major (claude-3-5-sonnet).
pub(super) fn normalize_claude_family_bare_major(lower: &str) -> Option<String> {
    let parts: Vec<&str> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect();
    let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());

    for (idx, part) in parts.iter().enumerate() {
        if !CLAUDE_FAMILY_TOKENS.contains(part) {
            continue;
        }
        if let Some(major) = parts
            .get(idx + 1)
            .copied()
            .filter(|p| is_modern_claude_major(p))
        {
            if parts.get(idx + 2).is_none_or(|next| !all_digits(next)) {
                return Some(format!("claude-{part}-{major}"));
            }
        }
        if idx >= 1
            && is_modern_claude_major(parts[idx - 1])
            && (idx < 2 || !all_digits(parts[idx - 2]))
        {
            return Some(format!("claude-{part}-{}", parts[idx - 1]));
        }
    }

    None
}

/// True if the id carries a delimited modern `major(-|.)minor` version
/// (4-6, 4.8, 5-0, 4-60, 4-20250514). Generalizes the former
/// `contains_delimited_major_minor(lower, '4')` checks across all modern
/// majors so the never-degrade contract also covers major 5 and up.
pub(in crate::pricing::lookup) fn contains_delimited_modern_major_minor(haystack: &str) -> bool {
    ('4'..='9').any(|major| contains_delimited_major_minor(haystack, major))
}

/// The version-pinned canonical key a Claude id requests, used to veto
/// fuzzy/stripped resolutions that would land on a different version.
///
/// - An explicit single-digit minor (claude-sonnet-4-7) always pins; this is
///   main's opus-only minor guard generalized across families.
/// - A bare major pins from major 5 up (claude-opus-5 must never bill as any
///   opus 4.x key). Bare major 4 is deliberately left unpinned to preserve
///   the long-standing behavior of e.g. `claude-opus-4` resolving to a
///   dated or regional 4.x dataset key.
pub(in crate::pricing::lookup) fn requested_claude_version(lower: &str) -> Option<String> {
    if let Some(model) = normalize_claude_family_minor(lower) {
        return Some(model);
    }
    normalize_claude_family_bare_major(lower).filter(|model| !model.ends_with("-4"))
}

fn is_single_digit_minor(value: &str) -> bool {
    value.len() == 1 && value.as_bytes()[0].is_ascii_digit() && value.as_bytes()[0] != b'0'
}

pub(in crate::pricing::lookup) fn normalize_version_separator(model_id: &str) -> Option<String> {
    let mut result = String::with_capacity(model_id.len());
    let chars: Vec<char> = model_id.chars().collect();
    let mut changed = false;

    for i in 0..chars.len() {
        if chars[i] == '-'
            && i > 0
            && i < chars.len() - 1
            && chars[i - 1].is_ascii_digit()
            && chars[i + 1].is_ascii_digit()
        {
            let is_multi_digit_before = i >= 2 && chars[i - 2].is_ascii_digit();
            let is_multi_digit_after = i + 2 < chars.len() && chars[i + 2].is_ascii_digit();
            let looks_like_date = is_multi_digit_before || is_multi_digit_after;

            if looks_like_date {
                result.push(chars[i]);
            } else {
                result.push('.');
                changed = true;
            }
        } else {
            result.push(chars[i]);
        }
    }

    if changed {
        Some(result)
    } else {
        None
    }
}
