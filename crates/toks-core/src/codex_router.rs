//! Installation and runtime support for routing local Codex model traffic.

pub mod account_activation;
pub(crate) mod codex_binary;
#[cfg(test)]
mod codex_binary_tests;
mod codex_config;
pub(crate) mod credentials;
#[cfg(test)]
mod credentials_tests;
mod deployment_status;
mod handoff;
pub(crate) mod host;
mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
pub mod proxy;
mod resume;
mod systemd;
mod thread_source;
#[cfg(test)]
mod thread_source_tests;
pub mod thread_titles;
#[cfg(test)]
mod thread_titles_tests;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::storage::StoreUpdate;

pub use deployment_status::{
    RouterDeploymentStatus, RouterGenerationRole, RouterGenerationSummary,
};
pub use lifecycle::{disable, enable, install_router_service, install_router_service_for};

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
        service_installed: systemd::is_installed(),
        service_active: systemd::is_active() && systemd::is_ready(),
    }
}

pub fn deployment_status(
    runtime: &crate::rotation::RotationRuntime,
) -> Result<RouterDeploymentStatus> {
    deployment_status::load(runtime)
}

pub(crate) fn acknowledge_banked_reset(account: &crate::accounts::AccountId) -> Result<()> {
    let store = crate::rotation::RotationRuntimeStore::discover()?;
    acknowledge_banked_reset_in(&store, account)
}

fn acknowledge_banked_reset_in(
    store: &crate::rotation::RotationRuntimeStore,
    account: &crate::accounts::AccountId,
) -> Result<()> {
    store.update(|runtime| {
        runtime.banked_reset_consumed(account);
        StoreUpdate::Changed(())
    })
}

pub fn router_executable_for(app_executable: &Path) -> Result<PathBuf> {
    let parent = app_executable
        .parent()
        .context("Toks executable has no parent directory")?;
    Ok(parent.join("toks-router"))
}

/// Replaces the systemd entry process with an exact-environment coordinator.
pub fn launch_router_host() -> Result<()> {
    systemd::launch_host()
}

/// Replaces the systemd entry process with an exact-environment resume supervisor.
pub fn launch_router_resume_supervisor() -> Result<()> {
    systemd::launch_resume_supervisor()
}

/// Replaces a transient systemd entry process with an exact-environment task.
pub fn launch_router_resume_task(encoded: &str) -> Result<()> {
    resume::launch_task(encoded)
}

pub async fn run_router() -> Result<()> {
    let runtime = proxy::RouterRuntimeHandle::discover()?;
    proxy::serve(runtime).await
}

/// Runs the restartable coordinator. Transport workers outlive this process.
pub async fn run_router_host() -> Result<()> {
    let runtime = proxy::RouterRuntimeHandle::discover()?;
    tokio::spawn(proxy::heartbeat(runtime.clone()));
    host::run_coordinator(runtime).await
}

/// Runs the task-resume supervisor independently from the coordinator.
pub async fn run_resume_supervisor() -> Result<()> {
    resume::run_supervisor().await
}

/// Runs one resume attempt inside its independently owned task unit.
pub async fn run_resume_task(attempt: &str, thread: &str, cwd: PathBuf) -> Result<()> {
    resume::run_task(attempt, crate::rotation::ThreadId::new(thread), cwd).await
}

/// Runs one independently managed transport-worker generation.
pub async fn run_router_worker(generation: u64) -> Result<()> {
    anyhow::ensure!(generation != 0, "router generation must be nonzero");
    host::run_worker(host::GenerationId::from_raw(generation)).await
}

/// Replaces this process with a worker using its persisted generation contract.
pub fn launch_router_worker(generation: u64, contract: &Path) -> Result<()> {
    anyhow::ensure!(generation != 0, "router generation must be nonzero");
    systemd::launch_generation(contract, generation)
}

#[cfg(test)]
mod tests;
