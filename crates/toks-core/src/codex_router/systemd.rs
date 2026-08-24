use super::ROUTER_PORT;
use anyhow::{Context, Result};
pub(in crate::codex_router) use command::coordinator_main_pid_until;
use command::{
    coordinator_matches_until, execute_until, health_check, is_unit_active, is_unit_active_until,
    resume_matches_until,
};
use plan::{install_plan, InstallFacts};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
mod command;
mod install_receipt;
mod launch_contract;
mod lifecycle_lock;
mod plan;
mod readiness;
#[cfg(test)]
mod readiness_tests;
mod receipt;
mod resume_unit;
mod runtime_process;
#[cfg(test)]
mod runtime_process_tests;
mod socket_contract;
#[cfg(test)]
mod socket_contract_tests;
mod uninstall;
mod units;
pub(in crate::codex_router) use launch_contract::coordinator_process_contract;
pub(super) use launch_contract::worker_process_contract;
pub(super) use launch_contract::{launch as launch_generation, stage as stage_generation};
pub(in crate::codex_router) use lifecycle_lock::LifecycleGuard;
#[cfg(test)]
pub(super) use receipt::build_id as deployment_build_id;
pub(in crate::codex_router) use receipt::deployment_state_path;
pub(in crate::codex_router) use runtime_process::{
    allowed_environment, exact_command, validate_allowed_environment,
};
pub(super) use units::render_socket_unit;
pub(in crate::codex_router) use units::UnitEnvironment;
#[cfg(test)]
pub(super) use units::{render_service_unit, render_worker_unit, COORDINATOR_STOP_TIMEOUT_SECONDS};

#[cfg(test)]
pub(super) use command::healthy_response;
#[cfg(test)]
mod resume_tests;
#[cfg(test)]
mod tests;

const UNIT_NAME: &str = "toks-router.service";
const SOCKET_NAME: &str = "toks-router.socket";
const WORKER_NAME: &str = "toks-router-worker@.service";
const RESUME_NAME: &str = "toks-router-resume.service";
const PENDING_NAME: &str = ".toks-router-install-pending.json";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(30);

pub(in crate::codex_router) fn launch_host() -> Result<()> {
    runtime_process::launch_static("host", true)
}

pub(in crate::codex_router) fn launch_resume_supervisor() -> Result<()> {
    runtime_process::launch_static("resume-supervisor", false)
}
pub(super) const fn launch_contract_name() -> &'static str {
    launch_contract::CONTRACT_NAME
}
pub(super) fn install(
    _lifecycle: &LifecycleGuard,
    executable: &Path,
    codex_executable: &Path,
) -> Result<()> {
    let deadline = Instant::now() + INSTALL_TIMEOUT;
    let socket = render_socket_unit();
    let socket_active = is_unit_active_until(SOCKET_NAME, deadline)?;
    if socket_active {
        socket_contract::ensure_active_candidate_until(deadline)?;
    }
    let rendered = receipt::render_units(executable, codex_executable)?;
    let executable = rendered.executable.clone();
    let candidate = rendered.build;
    let process_environment = rendered.process_environment;
    let artifact_root = receipt::artifact_root()?;
    let units = [
        (UNIT_NAME, rendered.coordinator),
        (SOCKET_NAME, socket),
        (WORKER_NAME, rendered.worker),
        (RESUME_NAME, rendered.resume),
    ];
    let changes = units.each_ref().map(|(name, unit)| {
        named_unit_path(name)
            .is_ok_and(|path| fs::read_to_string(path).ok().as_deref() != Some(unit.as_str()))
    });
    launch_contract::persist(&artifact_root, &rendered.contract, &candidate)?;
    let pending_path = named_unit_path(PENDING_NAME)?;
    let mut pending = install_receipt::load(&pending_path);
    let deployment_state = receipt::deployment_state_path()?;
    let retry_failed = receipt::failed_candidate(&deployment_state, &candidate)?;
    if retry_failed {
        crate::codex_router::host::request_retry(&deployment_state, &candidate)?;
    }
    pending.record_changes(changes, retry_failed);
    if pending.requires_action() {
        install_receipt::save(&pending_path, &pending)?;
    }
    for ((name, unit), changed) in units.iter().zip(changes) {
        if changed {
            crate::storage::write_private_atomic(
                &named_unit_path(name)?,
                unit.as_bytes(),
                "router user unit",
            )?;
        }
    }
    let facts = InstallFacts {
        service_active: is_unit_active_until(UNIT_NAME, deadline)?,
        socket_active,
        resume_active: is_unit_active_until(RESUME_NAME, deadline)?,
        resume_matches: resume_matches_until(&executable, &process_environment, deadline)?,
        coordinator_matches: coordinator_matches_until(
            &executable,
            &process_environment,
            deadline,
        )?,
        restart_coordinator: pending.restart_coordinator,
        restart_resume: pending.restart_resume,
    };
    for action in install_plan(facts) {
        if matches!(
            action,
            plan::Action::StartCoordinator | plan::Action::RestartCoordinator
        ) {
            socket_contract::ensure_active_candidate_until(deadline)?;
        }
        execute_until(action, deadline)?;
        if pending.completed(action) {
            install_receipt::save(&pending_path, &pending)?;
        }
    }
    readiness::wait(&executable, &candidate, &process_environment, deadline)?;
    remove_if_exists(&pending_path)
}

#[cfg(test)]
pub(crate) use launch_contract::persist_test as persist_test_launch_contract;

pub(super) fn uninstall(lifecycle: &LifecycleGuard) -> Result<()> {
    uninstall::run(lifecycle)
}

pub(super) fn is_active() -> bool {
    is_unit_active(UNIT_NAME)
}

pub(super) fn is_ready() -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], ROUTER_PORT));
    health_check(address).is_ok()
}

pub(super) fn is_installed() -> bool {
    [UNIT_NAME, SOCKET_NAME, WORKER_NAME, RESUME_NAME]
        .into_iter()
        .all(|name| named_unit_path(name).is_ok_and(|path| path.is_file()))
}

pub(super) fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("removing router service artifact"),
    }
}

pub(super) fn named_unit_path(name: &str) -> Result<PathBuf> {
    dirs::config_dir()
        .map(|root| root.join("systemd/user").join(name))
        .context("no local configuration directory")
}
