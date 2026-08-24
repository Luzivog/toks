use super::super::litellm::ModelPricing;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Clone)]
pub(super) struct CachedResult {
    pub(super) pricing: ModelPricing,
    pub(super) source: String,
    pub(super) matched_key: String,
}

pub(super) struct KeyModelPart {
    pub(super) key: String,
    pub(super) lower_model_part: String,
}

pub(super) struct ProviderScopedModelPath<'a> {
    pub(super) provider: &'a str,
    pub(super) terminal_model_id: &'a str,
}

pub struct PricingLookup {
    pub(super) litellm: HashMap<String, ModelPricing>,
    pub(super) openrouter: HashMap<String, ModelPricing>,
    pub(super) cursor: HashMap<String, ModelPricing>,
    pub(super) sakana: HashMap<String, ModelPricing>,
    pub(super) models_dev: HashMap<String, ModelPricing>,
    pub(super) litellm_keys: Vec<String>,
    pub(super) openrouter_keys: Vec<String>,
    pub(super) litellm_key_parts: Vec<KeyModelPart>,
    pub(super) openrouter_key_parts: Vec<KeyModelPart>,
    pub(super) models_dev_key_parts: Vec<KeyModelPart>,
    pub(super) litellm_lower: HashMap<String, String>,
    pub(super) openrouter_lower: HashMap<String, String>,
    pub(super) models_dev_lower: HashMap<String, String>,
    pub(super) openrouter_model_part: HashMap<String, String>,
    pub(super) models_dev_model_part: HashMap<String, String>,
    pub(super) cursor_lower: HashMap<String, String>,
    pub(super) sakana_lower: HashMap<String, String>,
    pub(super) lookup_cache: RwLock<HashMap<String, Option<CachedResult>>>,
}

pub struct LookupResult {
    pub pricing: ModelPricing,
    pub source: String,
    pub matched_key: String,
}
