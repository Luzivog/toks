//! Installation and runtime support for routing local Codex model traffic.

mod codex_binary;
mod codex_config;
pub(crate) mod credentials;
pub mod proxy;
mod resume;
mod systemd;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ROUTER_PORT: u16 = 47_837;
pub const ROUTER_BASE_URL: &str = "http://127.0.0.1:47837/backend-api/codex";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouterInstallStatus {
    pub configured: bool,
    pub service_installed: bool,
    pub service_active: bool,
}

pub fn status() -> RouterInstallStatus {
    RouterInstallStatus {
        configured: codex_config::is_configured().unwrap_or(false),
        service_installed: systemd::unit_path().is_ok_and(|path| path.is_file()),
        service_active: systemd::is_active(),
    }
}

/// Install and start the router before directing new Codex processes to it.
pub fn enable(router_executable: &Path) -> Result<()> {
    if !router_executable.is_file() {
        anyhow::bail!(
            "Codex router executable was not found at {}",
            router_executable.display()
        );
    }
    systemd::install(router_executable, &codex_binary::discover()?)?;
    set_enabled(true)?;
    systemd::wait_until_ready()?;
    codex_config::configure()?;
    Ok(())
}

/// Stop routing Codex traffic while leaving the startup unit available.
pub fn bypass() -> Result<()> {
    codex_config::restore()
}

/// Restore Codex configuration and remove the background user service.
pub fn disable() -> Result<()> {
    codex_config::restore()?;
    set_enabled(false)?;
    systemd::uninstall()
}

fn set_enabled(enabled: bool) -> Result<()> {
    let store = crate::rotation::RotationSettingsStore::discover()?;
    let mut settings = store.load()?;
    settings.reconcile(&credentials::account_ids());
    settings.set_enabled(enabled);
    store.save(&settings)
}

pub fn router_executable_for(app_executable: &Path) -> Result<PathBuf> {
    let parent = app_executable
        .parent()
        .context("Toks executable has no parent directory")?;
    Ok(parent.join("toks-router"))
}

pub async fn run_router() -> Result<()> {
    let runtime = proxy::RouterRuntimeHandle::discover()?;
    tokio::spawn(resume::run(runtime.clone()));
    proxy::serve(runtime).await
}

#[cfg(test)]
mod tests;
