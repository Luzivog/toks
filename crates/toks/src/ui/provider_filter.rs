use std::borrow::Cow;

use toks_core::history::{merge_source_usage, HistorySnapshot, SourceHistory, UsageSeries};
use toks_core::{ClientId, ProviderVisibility, USAGE_PROVIDERS};

pub(super) fn visible_source<'a>(
    history: &'a HistorySnapshot,
    provider: ClientId,
    visibility: &ProviderVisibility,
) -> Option<&'a SourceHistory> {
    visibility
        .is_visible(provider)
        .then(|| history.source(provider.as_str()))
        .flatten()
}

pub(super) fn visible_sources<'a>(
    history: &'a HistorySnapshot,
    visibility: &'a ProviderVisibility,
) -> impl Iterator<Item = &'a SourceHistory> + 'a {
    history.sources.iter().filter(move |source| {
        ClientId::from_str(&source.client).is_some_and(|provider| {
            USAGE_PROVIDERS.contains(&provider) && visibility.is_visible(provider)
        })
    })
}

pub(super) fn visible_usage<'a>(
    history: &'a HistorySnapshot,
    visibility: &ProviderVisibility,
) -> Cow<'a, UsageSeries> {
    if visibility.visible_count() == USAGE_PROVIDERS.len() {
        Cow::Borrowed(&history.usage)
    } else {
        Cow::Owned(merge_source_usage(visible_sources(history, visibility)))
    }
}
