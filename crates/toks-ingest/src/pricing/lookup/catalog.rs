use super::super::litellm::ModelPricing;
use super::cost::has_any_usable_pricing;
use super::select::prefers_model_part_key;
use super::state::{KeyModelPart, PricingLookup};
use std::collections::HashMap;
use std::sync::RwLock;

impl PricingLookup {
    pub fn new(
        litellm: HashMap<String, ModelPricing>,
        openrouter: HashMap<String, ModelPricing>,
        cursor: HashMap<String, ModelPricing>,
    ) -> Self {
        // Bare `new` keeps the legacy 3-source shape (no Sakana built-in
        // overrides); production wiring goes through `new_with_models_dev`
        // which threads the Sakana map alongside Cursor.
        Self::new_with_models_dev(litellm, openrouter, cursor, HashMap::new(), HashMap::new())
    }

    // @keep: the omission of cursor/sakana is the whole point and reads like a bug otherwise.
    /// True when at least one *fetchable* upstream dataset loaded.
    ///
    /// The `cursor` and `sakana` tables are compiled-in constants that are
    /// present on every run, so they are deliberately not consulted: counting
    /// them would report healthy pricing during a total upstream outage, which
    /// is exactly the condition callers use this to detect.
    pub fn has_upstream_dataset(&self) -> bool {
        !self.litellm.is_empty() || !self.openrouter.is_empty() || !self.models_dev.is_empty()
    }

    pub fn new_with_models_dev(
        litellm: HashMap<String, ModelPricing>,
        openrouter: HashMap<String, ModelPricing>,
        cursor: HashMap<String, ModelPricing>,
        sakana: HashMap<String, ModelPricing>,
        models_dev: HashMap<String, ModelPricing>,
    ) -> Self {
        let mut litellm_keys: Vec<String> = litellm.keys().cloned().collect();
        litellm_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

        let mut openrouter_keys: Vec<String> = openrouter.keys().cloned().collect();
        openrouter_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

        let mut models_dev_keys: Vec<String> = models_dev.keys().cloned().collect();
        models_dev_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

        let mut litellm_lower = HashMap::with_capacity(litellm.len());
        for key in &litellm_keys {
            litellm_lower.insert(key.to_lowercase(), key.clone());
        }

        let mut openrouter_lower = HashMap::with_capacity(openrouter.len());
        let mut openrouter_model_part = HashMap::with_capacity(openrouter.len());
        for key in &openrouter_keys {
            let lower = key.to_lowercase();
            openrouter_lower.insert(lower.clone(), key.clone());
            if let Some(model_part) = lower.split('/').next_back() {
                if model_part != lower {
                    openrouter_model_part.insert(model_part.to_string(), key.clone());
                }
            }
        }

        let mut models_dev_lower = HashMap::with_capacity(models_dev.len());
        let mut models_dev_model_part: HashMap<String, String> =
            HashMap::with_capacity(models_dev.len());
        for key in &models_dev_keys {
            let lower = key.to_lowercase();
            models_dev_lower.insert(lower.clone(), key.clone());
            // Only priced entries enter the model-part index: the
            // deterministic anthropic-first preference must choose among
            // keys that can actually price usage, otherwise an unpriced
            // `anthropic/<model>` row would shadow a priced reseller row
            // and bill the model at zero cost. (The models.dev loader only
            // emits entries with input+output costs — see
            // `models_dev::cost_to_pricing` — but this constructor is
            // public, so the index guards itself too.)
            if !models_dev.get(key).is_some_and(has_any_usable_pricing) {
                continue;
            }
            if let Some(model_part) = lower.split('/').next_back() {
                if model_part != lower {
                    match models_dev_model_part.entry(model_part.to_string()) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            if prefers_model_part_key(key, entry.get()) {
                                entry.insert(key.clone());
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(key.clone());
                        }
                    }
                }
            }
        }

        let mut cursor_lower = HashMap::with_capacity(cursor.len());
        for key in cursor.keys() {
            cursor_lower.insert(key.to_lowercase(), key.clone());
        }

        let mut sakana_lower = HashMap::with_capacity(sakana.len());
        for key in sakana.keys() {
            sakana_lower.insert(key.to_lowercase(), key.clone());
        }

        let build_key_parts = |keys: &[String]| -> Vec<KeyModelPart> {
            keys.iter()
                .map(|key| {
                    let lower = key.to_lowercase();
                    let model_part = lower.split('/').next_back().unwrap_or(&lower).to_string();
                    KeyModelPart {
                        key: key.clone(),
                        lower_model_part: model_part,
                    }
                })
                .collect()
        };

        let litellm_key_parts = build_key_parts(&litellm_keys);
        let openrouter_key_parts = build_key_parts(&openrouter_keys);
        let models_dev_key_parts = build_key_parts(&models_dev_keys);

        Self {
            litellm,
            openrouter,
            cursor,
            sakana,
            models_dev,
            litellm_keys,
            openrouter_keys,
            litellm_key_parts,
            openrouter_key_parts,
            models_dev_key_parts,
            litellm_lower,
            openrouter_lower,
            models_dev_lower,
            openrouter_model_part,
            models_dev_model_part,
            cursor_lower,
            sakana_lower,
            lookup_cache: RwLock::new(HashMap::with_capacity(64)),
        }
    }
}
