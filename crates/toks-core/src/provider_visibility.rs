use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::ClientId;

const DOCUMENT_VERSION: u8 = 1;

pub const USAGE_PROVIDERS: [ClientId; 3] = [ClientId::Codex, ClientId::Claude, ClientId::OpenCode];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderVisibility {
    hidden: HashSet<ClientId>,
}

impl ProviderVisibility {
    pub fn is_visible(&self, provider: ClientId) -> bool {
        !self.hidden.contains(&provider)
    }

    pub fn visible_count(&self) -> usize {
        USAGE_PROVIDERS
            .iter()
            .filter(|provider| self.is_visible(**provider))
            .count()
    }

    pub fn can_hide(&self, provider: ClientId) -> bool {
        USAGE_PROVIDERS.contains(&provider) && self.is_visible(provider) && self.visible_count() > 1
    }

    /// Change one usage provider without allowing the visible set to become empty.
    pub fn set_visible(&mut self, provider: ClientId, visible: bool) -> bool {
        if !USAGE_PROVIDERS.contains(&provider) {
            return false;
        }
        if visible {
            self.hidden.remove(&provider)
        } else if self.can_hide(provider) {
            self.hidden.insert(provider)
        } else {
            false
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderVisibility {
    version: u8,
    hidden_providers: Vec<String>,
}

pub fn load_provider_visibility() -> ProviderVisibility {
    crate::paths::provider_visibility_file()
        .and_then(|path| try_load_at(&path))
        .unwrap_or_default()
}

pub fn save_provider_visibility(visibility: &ProviderVisibility) -> Result<()> {
    save_at(&crate::paths::provider_visibility_file()?, visibility)
}

#[cfg(test)]
pub(crate) fn load_at(path: &Path) -> ProviderVisibility {
    try_load_at(path).unwrap_or_default()
}

fn try_load_at(path: &Path) -> Result<ProviderVisibility> {
    let bytes = fs::read(path).context("reading provider visibility")?;
    let stored: StoredProviderVisibility =
        serde_json::from_slice(&bytes).context("parsing provider visibility")?;
    if stored.version != DOCUMENT_VERSION {
        bail!("unsupported provider visibility version {}", stored.version);
    }

    let mut visibility = ProviderVisibility::default();
    for slug in stored.hidden_providers {
        let Some(provider) = ClientId::from_str(&slug) else {
            bail!("unknown usage provider {slug}");
        };
        if !USAGE_PROVIDERS.contains(&provider) {
            bail!("unknown usage provider {slug}");
        }
        visibility.hidden.insert(provider);
    }
    if visibility.visible_count() == 0 {
        bail!("provider visibility cannot hide every provider");
    }
    Ok(visibility)
}

pub(crate) fn save_at(path: &Path, visibility: &ProviderVisibility) -> Result<()> {
    let parent = path
        .parent()
        .context("provider visibility path has no parent")?;
    fs::create_dir_all(parent).context("creating Toks data directory")?;
    crate::storage::restrict_directory(parent)?;
    let hidden_providers = USAGE_PROVIDERS
        .iter()
        .filter(|provider| !visibility.is_visible(**provider))
        .map(|provider| provider.as_str().to_owned())
        .collect();
    let bytes = serde_json::to_vec_pretty(&StoredProviderVisibility {
        version: DOCUMENT_VERSION,
        hidden_providers,
    })?;
    crate::storage::write_private_atomic(path, &bytes, "provider visibility")
}
