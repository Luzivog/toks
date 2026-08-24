use super::provider::{build_lookup_cache_key, normalize_provider_hint};
use super::state::{CachedResult, LookupResult, PricingLookup};

const MAX_LOOKUP_CACHE_ENTRIES: usize = 512;

impl PricingLookup {
    pub fn lookup(&self, model_id: &str) -> Option<LookupResult> {
        self.lookup_with_provider(model_id, None)
    }

    pub fn lookup_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let provider_id = normalize_provider_hint(provider_id);
        let cache_key = build_lookup_cache_key(model_id, provider_id);
        if let Some(cached) = self
            .lookup_cache
            .read()
            .ok()
            .and_then(|c| c.get(&cache_key).cloned())
        {
            return cached.map(|c| LookupResult {
                pricing: c.pricing,
                source: c.source,
                matched_key: c.matched_key,
            });
        }

        let result = self.lookup_with_source_and_provider(model_id, None, provider_id);

        if let Ok(mut cache) = self.lookup_cache.write() {
            if cache.len() >= MAX_LOOKUP_CACHE_ENTRIES {
                // Evict ~25% of entries instead of clearing everything.
                // This avoids a thundering-herd cache miss storm that happens
                // when clear() wipes all entries at once.
                let evict_count = cache.len() / 4;
                let keys_to_remove: Vec<String> = cache.keys().take(evict_count).cloned().collect();
                for key in keys_to_remove {
                    cache.remove(&key);
                }
            }
            cache.insert(
                cache_key,
                result.as_ref().map(|r| CachedResult {
                    pricing: r.pricing.clone(),
                    source: r.source.clone(),
                    matched_key: r.matched_key.clone(),
                }),
            );
        }

        result
    }
}
