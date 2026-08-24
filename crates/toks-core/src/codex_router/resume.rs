//! Durable supervision for Codex tasks released from the waiting queue.

use std::time::Duration;

use anyhow::{Context, Result};

use super::proxy::RouterRuntimeHandle;
use crate::rotation::UnixMillis;

mod selection;
mod state;
mod supervisor;
mod systemd_tasks;
mod task_command;
mod workspace;
#[cfg(test)]
mod workspace_tests;

use state::ResumeStore;
use supervisor::Supervisor;
use systemd_tasks::SystemdTasks;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Runs independently from the router coordinator. Transient task units own
/// resumed Codex processes, so replacing this supervisor does not stop them.
pub(super) async fn run_supervisor() -> Result<()> {
    let store = ResumeStore::discover()?;
    let _instance = store.acquire_supervisor_lock()?;
    let runtime = RouterRuntimeHandle::discover()?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let codex = super::codex_binary::discover()?;
    let mut supervisor = Supervisor::new(store, runtime, SystemdTasks::new(executable, codex)?)?;
    loop {
        if let Err(error) = supervisor.tick(UnixMillis::now()) {
            eprintln!("{}", tick_error_message(&error));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn tick_error_message(error: &anyhow::Error) -> String {
    format!("toks resume supervisor tick failed: {error:#}")
}

/// Runs in an independently managed transient service and records the command
/// outcome before exiting. The unit remains an outcome authority if receipt
/// storage itself fails.
pub(super) async fn run_task(
    attempt: &str,
    thread: crate::rotation::ThreadId,
    cwd: std::path::PathBuf,
) -> Result<()> {
    let store = ResumeStore::discover()?;
    state::validate_attempt_id(attempt)?;
    let state = store.load()?;
    let cwd = task_workspace(&state, attempt, &thread, cwd)?;
    let status = task_command::run_codex(attempt, thread, cwd).await;
    let success = status.as_ref().is_ok_and(|status| status.success());
    let recorded = store.record_outcome(attempt, success);
    match (status, recorded) {
        (Ok(status), _) if status.success() => Ok(()),
        (Ok(status), Ok(())) => anyhow::bail!(task_failure_message(status)),
        (Err(error), Ok(())) => Err(error),
        (Ok(status), Err(recording)) => anyhow::bail!(
            "{}; recording failed resumed task: {recording:#}",
            task_failure_message(status)
        ),
        (Err(task), Err(recording)) => anyhow::bail!(
            "resumed Codex task could not start: {task:#}; recording failed resumed task: {recording:#}"
        ),
    }
}

pub(super) fn launch_task(encoded: &str) -> Result<()> {
    systemd_tasks::launch::execute(encoded)
}

fn task_workspace(
    state: &state::ResumeState,
    attempt: &str,
    thread: &crate::rotation::ThreadId,
    cwd: std::path::PathBuf,
) -> Result<std::path::PathBuf> {
    let cwd = workspace::validate(cwd)?;
    let persisted = state
        .attempts
        .get(thread)
        .filter(|persisted| persisted.id == attempt)
        .context("resume attempt is no longer current")?;
    anyhow::ensure!(
        persisted.cwd == cwd,
        "resume workspace does not match attempt"
    );
    Ok(cwd)
}

fn task_failure_message(status: std::process::ExitStatus) -> String {
    format!("resumed Codex task exited unsuccessfully ({status})")
}

#[cfg(test)]
mod tests;
