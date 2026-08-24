fn canonicalize_provider_segment(segment: &str) -> Option<String> {
    let normalized = segment
        .trim()
        .trim_end_matches('/')
        .to_lowercase()
        .replace('-', "_");
    if normalized.starts_with('<') && normalized.ends_with('>') {
        return None;
    }

    let canonical = match normalized.as_str() {
        "" | "unknown" => return None,
        "x_ai" | "xai" => "xai",
        "z_ai" | "zai" => "zai",
        "moonshot" | "moonshotai" => "moonshotai",
        "meta" | "meta_llama" => "meta_llama",
        "azure" | "azure_ai" => "azure_ai",
        "anthropic" | "vertex" | "vertex_ai" => "anthropic",
        "together" | "together_ai" => "together_ai",
        "fireworks" | "fireworks_ai" => "fireworks_ai",
        "google" | "gemini" => "google",
        "openai" | "openai_codex" => "openai",
        "minimax" | "minimaxai" | "minimax_ai" => "minimax",
        "mistral" | "mistralai" => "mistralai",
        "ai21" => "ai21",
        // The `-ai` suffix is a spelling of the vendor, not a different
        // vendor. DeepSeek is the case that actually costs us: the live
        // datasets split the same model almost evenly between the two
        // spellings depending on who is reselling it —
        // `zenmux/deepseek/deepseek-v3.2-exp` and `kilo/deepseek/...` against
        // `nano-gpt/deepseek-ai/deepseek-v3.2-exp` and `siliconflow/...` — so
        // whether usage of one model carried the vendor tag `deepseek` or
        // `deepseek_ai` was decided by which reseller served it.
        //
        // Folding is only safe because the two spellings never name the same
        // row: no reseller in either dataset lists a model under both, and
        // `deepseek-ai` is never a top-level provider (it is the HuggingFace
        // org name, always a nested segment), so nothing here collapses two
        // separately-priced rows into one.
        "deepseek" | "deepseek_ai" => "deepseek",
        "novita" | "novita_ai" => "novita",
        "stepfun" | "stepfun_ai" => "stepfun",
        // A `-cn` suffix is NOT a spelling variant and must never be folded in
        // here, however much it looks like one. It marks a regional endpoint
        // with its own price sheet: `alibaba` and `alibaba-cn` share 45 models
        // and disagree on 41 of them, with `qwen-max` at $1.60/$6.40 against
        // $0.345/$1.377 — a 4.6x error in whichever direction the fold went.
        // `siliconflow` and `siliconflow-cn` disagree on 7 of 35. Adding those
        // arms to "finish the pattern" would silently misprice every user on
        // the endpoint that lost the fold.
        // For unknown segments, reject if they contain digits — those are
        // almost certainly model-name fragments (e.g., "gpt-4", "claude-3")
        // rather than provider identifiers.
        other if other.chars().any(|ch| ch.is_ascii_digit()) => return None,
        other => other,
    };

    Some(canonical.into())
}

pub fn canonical_provider(raw: &str) -> Option<String> {
    provider_tags(raw).into_iter().next()
}

pub fn provider_tags(raw: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut push = |segment: &str| {
        if let Some(tag) = canonicalize_provider_segment(segment) {
            if !tags.iter().any(|existing| existing == &tag) {
                tags.push(tag);
            }
        }
    };

    for segment in raw.trim().trim_end_matches('/').split('/') {
        push(segment);
        if segment.contains('.') {
            for dotted in segment.split('.') {
                push(dotted);
            }
        }
    }

    tags
}

pub fn key_provider_tags(dataset_key: &str) -> Vec<String> {
    let key_parts: Vec<&str> = dataset_key.split('/').collect();
    if key_parts.len() < 2 {
        return Vec::new();
    }

    let mut tags = Vec::new();
    let mut push_all = |value: &str| {
        for tag in provider_tags(value) {
            if !tags.iter().any(|existing| existing == &tag) {
                tags.push(tag);
            }
        }
    };

    for segment in &key_parts[..key_parts.len() - 1] {
        push_all(segment);
    }
    for dotted in key_parts[key_parts.len() - 1].split('.') {
        push_all(dotted);
    }

    tags
}

/// Provider segments a value names *verbatim*, with no alias folding.
///
/// Lowercased and underscore-normalized so `DeepSeek-AI`, `deepseek-ai` and
/// `deepseek_ai` compare equal, but `deepseek` and `deepseek_ai` do not.
fn raw_provider_segments(value: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut push = |segment: &str| {
        let normalized = segment
            .trim()
            .trim_end_matches('/')
            .to_lowercase()
            .replace('-', "_");
        if normalized.is_empty() || segments.iter().any(|existing| existing == &normalized) {
            return;
        }
        segments.push(normalized);
    };

    for segment in value.trim().trim_end_matches('/').split('/') {
        push(segment);
        if segment.contains('.') {
            for dotted in segment.split('.') {
                push(dotted);
            }
        }
    }

    segments
}

/// Whether `dataset_key` spells a vendor exactly the way `provider_id` does.
///
/// `canonicalize_provider_segment` folds spelling variants of one vendor
/// together (`deepseek-ai` -> `deepseek`) so a hint can reach rows that spell
/// the vendor the other way. That fold also widens the candidate pool: a
/// `deepseek` hint now matches both `novita/deepseek/<model>` and
/// `cloudflare/@cf/deepseek-ai/<model>`, which are two resellers with
/// different price sheets for the same weights. When the hinted vendor
/// publishes no first-party row for the model, nothing else in
/// `select_best_match` distinguishes those two, and the winner falls out of
/// dataset key ordering. This predicate is the tiebreak that keeps the fold
/// from re-rolling that choice: a row spelling the vendor exactly as the hint
/// does wins over one that only matches after folding.
pub(crate) fn matches_provider_spelling(dataset_key: &str, provider_id: &str) -> bool {
    let hint_segments = raw_provider_segments(provider_id);
    if hint_segments.is_empty() {
        return false;
    }

    let key_parts: Vec<&str> = dataset_key.split('/').collect();
    if key_parts.len() < 2 {
        return false;
    }

    // The final component is the model name, but an AWS-style id carries the
    // provider in a dotted prefix of it — `amazon-bedrock/us.deepseek.r1-v1:0`
    // is DeepSeek's row, not Amazon's own model. `key_provider_tags` already
    // splits that component on `.` for exactly this reason, so read the same
    // segments here: dropping them lets a `deepseek` hint miss the row that
    // spells the vendor its way and fall through to a differently spelled
    // reseller. Only the dotted *prefix* counts; the trailing piece is the
    // model name and is never a vendor spelling.
    let last = key_parts[key_parts.len() - 1];
    let dotted_provider_prefix = last.rsplit_once('.').map(|(prefix, _)| prefix);

    key_parts[..key_parts.len() - 1]
        .iter()
        .copied()
        .chain(dotted_provider_prefix)
        .flat_map(raw_provider_segments)
        .any(|key_segment| hint_segments.iter().any(|hint| hint == &key_segment))
}

pub fn matches_provider_hint(dataset_key: &str, provider_id: Option<&str>) -> bool {
    let Some(provider_id) = provider_id else {
        return false;
    };

    let hint_tags = provider_tags(provider_id);
    matches_provider_hint_with_tags(dataset_key, &hint_tags)
}

pub fn matches_provider_hint_with_tags(dataset_key: &str, hint_tags: &[String]) -> bool {
    if hint_tags.is_empty() {
        return false;
    }

    let key_tags = key_provider_tags(dataset_key);
    if key_tags.is_empty() {
        return false;
    }

    key_tags
        .iter()
        .any(|key_tag| hint_tags.iter().any(|hint_tag| hint_tag == key_tag))
}

fn contains_delimited(haystack: &str, needle: &str) -> bool {
    for (pos, _) in haystack.match_indices(needle) {
        let before_ok = pos == 0 || !haystack.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let after_pos = pos + needle.len();
        let after_ok =
            after_pos == haystack.len() || !haystack.as_bytes()[after_pos].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

pub fn inferred_provider_from_model(model: &str) -> Option<&'static str> {
    let lower = model.to_lowercase();

    // Ollama is a routing prefix, not part of the upstream model family. In
    // particular, matching the `llama` in `ollama/...` would label every
    // otherwise-unknown Ollama model as Meta. Re-run inference on the routed
    // model so known families retain their actual providers.
    if let Some(routed_model) = lower.strip_prefix("ollama/") {
        return inferred_provider_from_model(routed_model);
    }

    if lower.contains("claude")
        || lower.contains("anthropic")
        || contains_delimited(&lower, "opus")
        || contains_delimited(&lower, "sonnet")
        || contains_delimited(&lower, "haiku")
        || contains_delimited(&lower, "fable")
    {
        return Some("anthropic");
    }

    if lower.contains("gpt")
        || lower.contains("openai")
        || contains_delimited(&lower, "o1")
        || contains_delimited(&lower, "o3")
        || contains_delimited(&lower, "o4")
    {
        return Some("openai");
    }

    if lower.contains("gemini") || lower.contains("google") {
        return Some("google");
    }

    if lower.contains("grok") {
        return Some("xai");
    }

    if lower.contains("deepseek") {
        return Some("deepseek");
    }

    if lower.contains("minimax") {
        return Some("minimax");
    }

    if lower.contains("mistral") || lower.contains("mixtral") {
        return Some("mistral");
    }

    if lower.contains("llama") || contains_delimited(&lower, "meta") {
        return Some("meta");
    }

    if lower.contains("qwen") {
        return Some("qwen");
    }

    // Sakana's `fugu` / `fugu-ultra` model line. Bare `fugu` is intentionally
    // still mapped to the sakana provider here (provider identity is independent
    // of whether we can price the model — see build_sakana_overrides, which
    // deliberately does NOT price bare `fugu`).
    if lower.contains("fugu") {
        return Some("sakana");
    }

    // Kimi (Moonshot AI) — `kimi`, `kimi-k2.5`, `kimi-code` variants
    if contains_delimited(&lower, "kimi") {
        return Some("moonshotai");
    }
    // Kimi's own coding-plan catalog also serves bare `k2`/`k3`-style ids with
    // no `kimi` prefix at all (e.g. `k3`, `k3-256k` from the K3 coding-plan
    // model), so the `kimi` substring check above misses them. No other known
    // provider uses a bare, delimited `k2`/`k3` model id (checked against the
    // full litellm/models.dev/openrouter pricing datasets), so this is safe
    // without the `kimi` prefix.
    if contains_delimited(&lower, "k2") || contains_delimited(&lower, "k3") {
        return Some("moonshotai");
    }
    // MiMo (Xiaomi) — `mimo-v2.5` etc.
    if contains_delimited(&lower, "mimo") {
        return Some("xiaomi");
    }
    // GLM (Zhipu AI / Zai) — `glm-4.6`, `glm-5.2` etc.
    if contains_delimited(&lower, "glm") {
        return Some("zai");
    }

    None
}

#[cfg(test)]
#[path = "provider_identity_tests.rs"]
mod tests;
