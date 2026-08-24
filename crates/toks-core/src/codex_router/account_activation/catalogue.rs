use std::path::Path;

#[cfg(test)]
use std::path::PathBuf;

use serde::Deserialize;

use crate::rotation::UnixMillis;

const MAX_CACHE_AGE_MS: i64 = 24 * 60 * 60 * 1_000;
const MAX_CLOCK_SKEW_MS: i64 = 5 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModelChoice {
    pub(super) slug: Option<String>,
    pub(super) reasoning: String,
}

#[derive(Debug, Default, Deserialize)]
struct Cache {
    fetched_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    models: Vec<Model>,
}

#[derive(Debug, Deserialize)]
struct Model {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    supported_in_api: bool,
    #[serde(default = "last_priority")]
    priority: u64,
    #[serde(default)]
    supported_reasoning_levels: Vec<Reasoning>,
}

#[derive(Debug, Deserialize)]
struct Reasoning {
    #[serde(default)]
    effort: String,
}

pub(super) fn best_for_profile(config: &Path) -> ModelChoice {
    best_at(&config.join("models_cache.json"), UnixMillis::now().get()).unwrap_or_else(|| {
        ModelChoice {
            // With no fresh catalogue for this exact account, let its Codex CLI
            // choose the current default instead of imposing another account's model.
            slug: None,
            reasoning: "low".into(),
        }
    })
}

fn best_at(path: &Path, now_ms: i64) -> Option<ModelChoice> {
    let cache = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())?;
    let fetched_at_ms = cache.fetched_at?.timestamp_millis();
    if fetched_at_ms > now_ms.saturating_add(MAX_CLOCK_SKEW_MS)
        || now_ms.saturating_sub(fetched_at_ms) > MAX_CACHE_AGE_MS
    {
        return None;
    }
    cache
        .models
        .into_iter()
        .filter(|model| {
            model.visibility == "list" && model.supported_in_api && !model.slug.is_empty()
        })
        .filter_map(|model| {
            lowest(&model.supported_reasoning_levels).map(|reasoning| {
                (
                    model.priority,
                    ModelChoice {
                        slug: Some(model.slug),
                        reasoning,
                    },
                )
            })
        })
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, choice)| choice)
}

fn lowest(levels: &[Reasoning]) -> Option<String> {
    levels
        .iter()
        .filter_map(|level| rank(&level.effort).map(|rank| (rank, level.effort.as_str())))
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, effort)| effort.to_owned())
}

fn rank(effort: &str) -> Option<u8> {
    match effort {
        "none" => Some(0),
        "minimal" => Some(1),
        "low" => Some(2),
        "medium" => Some(3),
        "high" => Some(4),
        "xhigh" => Some(5),
        "max" => Some(6),
        "ultra" => Some(7),
        _ => None,
    }
}

const fn last_priority() -> u64 {
    u64::MAX
}

#[cfg(test)]
pub(super) fn best_for_test(path: PathBuf, now_ms: i64) -> Option<ModelChoice> {
    best_at(&path, now_ms)
}
