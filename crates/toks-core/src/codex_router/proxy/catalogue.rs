//! Which models accept the Fast service tier.
//!
//! The Codex CLI fetches a model catalogue from the backend and caches it at
//! `$CODEX_HOME/models_cache.json`. Each entry advertises the speed tiers that
//! model accepts:
//!
//! ```json
//! { "slug": "gpt-5.6-sol",
//!   "service_tiers": [{"id": "priority", "name": "Fast", ...}],
//!   "additional_speed_tiers": ["fast"] }
//! ```
//!
//! Reading the CLI's own cache keeps the eligible-model list in sync with
//! whatever the client knows without maintaining a hardcoded list here, and
//! without spending a request of our own. Models that cannot take a speed tier
//! (`gpt-5.3-codex-spark`, `gpt-5.4-mini`) carry an empty `service_tiers`.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// How long a parsed catalogue is reused before re-reading the file. The CLI
/// refreshes its cache on its own cadence; this only bounds our staleness.
const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
struct Cache {
    #[serde(default)]
    models: Vec<Model>,
}

#[derive(Debug, Deserialize)]
struct Model {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    service_tiers: Vec<ServiceTier>,
    #[serde(default)]
    additional_speed_tiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ServiceTier {
    #[serde(default)]
    id: String,
}

/// Lazily-read view of the CLI's model catalogue.
pub(super) struct Catalogue {
    path: Option<PathBuf>,
    cached: Mutex<Option<(Instant, Vec<Model>)>>,
}

impl Catalogue {
    pub fn discover() -> Self {
        Self::at(crate::limits::codex::codex_home().map(|home| home.join("models_cache.json")))
    }

    pub fn at(path: Option<PathBuf>) -> Self {
        Self {
            path,
            cached: Mutex::new(None),
        }
    }

    /// The Fast tier id this model accepts (`"priority"`), or `None` when the
    /// model advertises no speed tier or the catalogue cannot be read. Unknown
    /// means "do not inject": serving a turn at standard tier is always safe,
    /// while an unsupported tier risks failing the turn outright.
    pub fn fast_tier(&self, model: &str) -> Option<&'static str> {
        let mut cached = self.cached.lock().expect("model catalogue poisoned");
        if cached
            .as_ref()
            .is_none_or(|(read_at, _)| read_at.elapsed() >= CACHE_TTL)
        {
            *cached = Some((Instant::now(), self.read()));
        }
        let (_, models) = cached.as_ref().expect("catalogue populated above");
        let model = models.iter().find(|entry| entry.slug == model)?;
        (model
            .service_tiers
            .iter()
            .any(|tier| matches!(tier.id.as_str(), "fast" | "priority"))
            || model
                .additional_speed_tiers
                .iter()
                .any(|tier| tier == "fast"))
        .then_some("priority")
    }

    fn read(&self) -> Vec<Model> {
        let Some(path) = self.path.as_ref() else {
            return Vec::new();
        };
        std::fs::read(path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Cache>(&raw).ok())
            .map(|cache| cache.models)
            .unwrap_or_default()
    }
}
