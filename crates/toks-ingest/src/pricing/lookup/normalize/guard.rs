use super::claude::{claude_family, normalize_model_name};
use crate::pricing::lookup::LookupResult;

/// Veto for resolutions that violate the never-degrade contract:
/// cross-family (a sonnet id billed at an opus key), cross-version (a 4-7 id
/// billed at a 4-6 key, a major-5 id billed at a 4.x key), or any
/// modern-Claude resolution for an id whose `major-minor` version could not
/// be parsed (4-60, 5-0, dated forms). Exact dataset hits stay allowed: they
/// either normalize back to the requested version or, for unparseable
/// versions, do not normalize at all. Generalization of the former
/// `resolves_different_claude_opus_4_minor`.
pub(in crate::pricing::lookup) fn resolves_unsafe_claude_version(
    requested_family: Option<&'static str>,
    requested_version: Option<&str>,
    unparsed_modern_version: bool,
    result: &LookupResult,
) -> bool {
    let Some(requested_family) = requested_family else {
        return false;
    };
    let matched_lower = result.matched_key.to_lowercase();

    if claude_family(&matched_lower).is_some_and(|family| family != requested_family) {
        return true;
    }

    let resolved = normalize_model_name(&matched_lower);
    if let Some(requested_version) = requested_version {
        return resolved.is_some_and(|resolved| resolved != requested_version);
    }
    unparsed_modern_version && resolved.is_some()
}
