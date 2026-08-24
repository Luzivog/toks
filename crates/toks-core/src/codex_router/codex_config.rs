use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};

use super::ROUTER_BASE_URL;

const BACKUP_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBackup {
    version: u8,
    previous_base_url: Option<String>,
}

pub(super) fn configure() -> Result<()> {
    configure_at(&config_path()?, &backup_path()?)
}

pub(super) fn restore() -> Result<()> {
    restore_at(&config_path()?, &backup_path()?)
}

pub(super) fn is_configured() -> Result<bool> {
    let path = config_path()?;
    let document = read_document(&path)?;
    Ok(base_url(&document).as_deref() == Some(ROUTER_BASE_URL))
}

pub(super) fn configure_at(config: &Path, backup: &Path) -> Result<()> {
    let mut document = read_document(config)?;
    if base_url(&document).as_deref() == Some(ROUTER_BASE_URL) {
        return Ok(());
    }
    write_private_json(
        backup,
        &ConfigBackup {
            version: BACKUP_VERSION,
            previous_base_url: base_url(&document),
        },
    )?;
    document["openai_base_url"] = value(ROUTER_BASE_URL);
    write_private(config, document.to_string().as_bytes())
}

pub(super) fn restore_at(config: &Path, backup: &Path) -> Result<()> {
    if !backup.exists() {
        return Ok(());
    }
    let raw = fs::read(backup).context("reading saved Codex configuration")?;
    let saved: ConfigBackup =
        serde_json::from_slice(&raw).context("parsing saved Codex configuration")?;
    if saved.version != BACKUP_VERSION {
        anyhow::bail!("saved Codex configuration has an unsupported version");
    }
    let mut document = read_document(config)?;
    if base_url(&document).as_deref() == Some(ROUTER_BASE_URL) {
        match saved.previous_base_url {
            Some(url) => document["openai_base_url"] = value(url),
            None => {
                document.remove("openai_base_url");
            }
        }
        write_private(config, document.to_string().as_bytes())?;
    }
    fs::remove_file(backup).context("removing saved Codex configuration")
}

fn config_path() -> Result<PathBuf> {
    let root = crate::limits::codex::codex_home().context("no Codex home directory")?;
    Ok(root.join("config.toml"))
}

fn backup_path() -> Result<PathBuf> {
    crate::paths::codex_config_backup()
}

fn read_document(path: &Path) -> Result<DocumentMut> {
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    fs::read_to_string(path)
        .context("reading Codex configuration")?
        .parse::<DocumentMut>()
        .context("parsing Codex configuration")
}

fn base_url(document: &DocumentMut) -> Option<String> {
    document
        .get("openai_base_url")
        .and_then(|item| item.as_str())
        .map(str::to_string)
}

fn write_private_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_private(path, &bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::storage::write_private_atomic(path, bytes, "Codex configuration")
}
