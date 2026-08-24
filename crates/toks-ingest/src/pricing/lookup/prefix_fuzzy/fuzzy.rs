use super::super::select::select_best_match;
use super::super::{LookupResult, PricingLookup};

impl PricingLookup {
    pub(in crate::pricing::lookup) fn fuzzy_match_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let family = extract_model_family(model_id);
        let mut family_matches_list: Vec<&String> = Vec::new();

        for key in &self.litellm_keys {
            let lower_key = key.to_lowercase();
            if family_matches(&lower_key, &family) && contains_model_id(&lower_key, model_id) {
                family_matches_list.push(key);
            }
        }

        if let Some(result) =
            select_best_match(&family_matches_list, &self.litellm, "LiteLLM", provider_id)
        {
            return Some(result);
        }

        let mut all_matches: Vec<&String> = Vec::new();
        for key in &self.litellm_keys {
            let lower_key = key.to_lowercase();
            if contains_model_id(&lower_key, model_id) {
                all_matches.push(key);
            }
        }

        select_best_match(&all_matches, &self.litellm, "LiteLLM", provider_id)
    }

    pub(in crate::pricing::lookup) fn fuzzy_match_openrouter(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let family = extract_model_family(model_id);
        let mut family_matches_list: Vec<&String> = Vec::new();

        for key in &self.openrouter_keys {
            let lower_key = key.to_lowercase();
            let model_part = lower_key.split('/').next_back().unwrap_or(&lower_key);
            if family_matches(model_part, &family) && contains_model_id(model_part, model_id) {
                family_matches_list.push(key);
            }
        }

        if let Some(result) = select_best_match(
            &family_matches_list,
            &self.openrouter,
            "OpenRouter",
            provider_id,
        ) {
            return Some(result);
        }

        let mut all_matches: Vec<&String> = Vec::new();
        for key in &self.openrouter_keys {
            let lower_key = key.to_lowercase();
            let model_part = lower_key.split('/').next_back().unwrap_or(&lower_key);
            if contains_model_id(model_part, model_id) {
                all_matches.push(key);
            }
        }

        select_best_match(&all_matches, &self.openrouter, "OpenRouter", provider_id)
    }
}

fn extract_model_family(model_id: &str) -> String {
    let lower = model_id.to_lowercase();

    if lower.contains("gpt-5") {
        return "gpt-5".into();
    }
    if lower.contains("gpt-4.1") {
        return "gpt-4.1".into();
    }
    if lower.contains("gpt-4o") {
        return "gpt-4o".into();
    }
    if lower.contains("gpt-4") {
        return "gpt-4".into();
    }
    if lower.contains("o3") {
        return "o3".into();
    }
    if lower.contains("o4") {
        return "o4".into();
    }

    if lower.contains("opus") {
        return "opus".into();
    }
    if lower.contains("sonnet") {
        return "sonnet".into();
    }
    if lower.contains("haiku") {
        return "haiku".into();
    }
    if lower.contains("claude") {
        return "claude".into();
    }

    if lower.contains("gemini-3") {
        return "gemini-3".into();
    }
    if lower.contains("gemini-2.5") {
        return "gemini-2.5".into();
    }
    if lower.contains("gemini-2") {
        return "gemini-2".into();
    }
    if lower.contains("gemini") {
        return "gemini".into();
    }

    if lower.contains("llama") {
        return "llama".into();
    }
    if lower.contains("mistral") {
        return "mistral".into();
    }
    if lower.contains("deepseek") {
        return "deepseek".into();
    }
    if lower.contains("qwen") {
        return "qwen".into();
    }

    lower
        .split(['-', '_', '.'])
        .next()
        .unwrap_or(&lower)
        .to_string()
}

fn family_matches(key: &str, family: &str) -> bool {
    if family.is_empty() {
        return true;
    }
    key.contains(family)
}

fn contains_model_id(key: &str, model_id: &str) -> bool {
    if let Some(pos) = key.find(model_id) {
        let before_ok = pos == 0 || !key[..pos].chars().last().unwrap().is_alphanumeric();
        let after_pos = pos + model_id.len();
        let after_ok =
            after_pos == key.len() || !key[after_pos..].chars().next().unwrap().is_alphanumeric();
        before_ok && after_ok
    } else {
        false
    }
}
