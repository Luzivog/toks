use std::path::Path;

use anyhow::{Context, Result};

use super::{codex_binary, codex_config, credentials, systemd};
use crate::storage::StoreUpdate;

/// Install and start the router before directing new Codex processes to it.
pub fn enable(router_executable: &Path) -> Result<()> {
    let lifecycle = systemd::LifecycleGuard::acquire()?;
    enable_locked(
        &lifecycle,
        router_executable,
        |lifecycle, executable| systemd::install(lifecycle, executable, &codex_binary::discover()?),
        codex_config::configure,
    )
}

pub(super) fn enable_locked(
    lifecycle: &systemd::LifecycleGuard,
    router_executable: &Path,
    install: impl FnOnce(&systemd::LifecycleGuard, &Path) -> Result<()>,
    configure: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if !router_executable.is_file() {
        anyhow::bail!(
            "Codex router executable was not found at {}",
            router_executable.display()
        );
    }
    install(lifecycle, router_executable)?;
    set_enabled(true)?;
    configure()?;
    Ok(())
}

/// Restore Codex configuration and remove the background user service.
pub fn disable() -> Result<()> {
    let lifecycle = systemd::LifecycleGuard::acquire()?;
    disable_locked(&lifecycle, codex_config::restore, systemd::uninstall)
}

pub(super) fn disable_locked(
    lifecycle: &systemd::LifecycleGuard,
    restore: impl FnOnce() -> Result<()>,
    uninstall: impl FnOnce(&systemd::LifecycleGuard) -> Result<()>,
) -> Result<()> {
    restore()?;
    set_enabled(false)?;
    uninstall(lifecycle)
}

/// Reinstall and activate the service topology for this router artifact.
pub fn install_router_service() -> Result<()> {
    install_router_service_from(None)
}

/// Reinstall only if this process still owns the published installer link.
pub fn install_router_service_for(installed_link: &Path) -> Result<()> {
    install_router_service_from(Some(installed_link))
}

fn install_router_service_from(installed_link: Option<&Path>) -> Result<()> {
    let lifecycle = systemd::LifecycleGuard::acquire()?;
    install_router_service_if_enabled(
        &lifecycle,
        &std::env::current_exe()?,
        installed_link,
        |lifecycle, executable| systemd::install(lifecycle, executable, &codex_binary::discover()?),
    )
}

pub(super) fn install_router_service_if_enabled(
    lifecycle: &systemd::LifecycleGuard,
    executable: &Path,
    installed_link: Option<&Path>,
    install: impl FnOnce(&systemd::LifecycleGuard, &Path) -> Result<()>,
) -> Result<()> {
    let executable = executable
        .canonicalize()
        .context("canonicalizing router installer executable")?;
    if let Some(installed_link) = installed_link {
        let published = installed_link
            .canonicalize()
            .context("canonicalizing published router link")?;
        if executable != published {
            return Ok(());
        }
    }
    if crate::rotation::RotationSettingsStore::discover()?
        .load()?
        .enabled()
    {
        install(lifecycle, &executable)?;
    }
    Ok(())
}

fn set_enabled(enabled: bool) -> Result<()> {
    let store = crate::rotation::RotationSettingsStore::discover()?;
    store.update(|settings| {
        settings.reconcile(&credentials::account_ids());
        settings.set_enabled(enabled);
        StoreUpdate::Changed(())
    })
}
