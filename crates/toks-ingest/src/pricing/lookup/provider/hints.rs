use crate::provider_identity;

pub(in crate::pricing::lookup) fn normalize_provider_hint(
    provider_id: Option<&str>,
) -> Option<&str> {
    provider_id
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))
}

pub(in crate::pricing::lookup) fn build_lookup_cache_key(
    model_id: &str,
    provider_id: Option<&str>,
) -> String {
    match provider_id {
        Some(provider) if !provider.trim().is_empty() => {
            format!("{}|{}", provider.to_lowercase(), model_id.to_lowercase())
        }
        _ => model_id.to_lowercase(),
    }
}

pub(super) fn model_prefix_matches_provider(model_id: &str, provider_id: Option<&str>) -> bool {
    let Some(hint) = provider_id else {
        return true;
    };
    let Some(prefix) = model_id.split('/').next() else {
        return false;
    };
    let prefix_tag = provider_identity::canonical_provider(prefix);
    let hint_primary = provider_identity::canonical_provider(hint);
    match (prefix_tag, hint_primary) {
        (Some(p), Some(h)) => p == h,
        _ => false,
    }
}

pub(super) fn provider_hint_matches_scoped_provider(
    provider_id: Option<&str>,
    scoped_provider: &str,
) -> bool {
    let Some(provider_id) = provider_id else {
        return true;
    };

    let scoped_tags = provider_identity::provider_tags(scoped_provider);
    let hint_tags = provider_identity::provider_tags(provider_id);
    !scoped_tags.is_empty()
        && scoped_tags
            .iter()
            .any(|scoped| hint_tags.iter().any(|hint| hint == scoped))
}

pub(super) fn provider_prefix_matches_scoped_provider(
    prefix: &str,
    scoped_tags: &[String],
) -> bool {
    if scoped_tags.is_empty() {
        return false;
    }

    provider_identity::provider_tags(prefix.trim_end_matches('/'))
        .iter()
        .any(|prefix_tag| scoped_tags.iter().any(|scoped| scoped == prefix_tag))
}
