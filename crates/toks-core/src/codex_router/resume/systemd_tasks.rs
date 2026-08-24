use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

use super::state::ResumeAttempt;
use super::supervisor::{TaskState, TaskUnits};

const UNIT_PREFIX: &str = "toks-router-resume-task-";

mod control;
pub(super) mod launch;
#[cfg(test)]
use control::bounded_output_with_timeout;
use launch::{launch_arguments, TaskEnvironment};

pub(super) struct SystemdTasks {
    executable: PathBuf,
    environment: TaskEnvironment,
}

impl SystemdTasks {
    pub(super) fn new(executable: PathBuf, codex: PathBuf) -> Result<Self> {
        Ok(Self {
            executable,
            environment: TaskEnvironment::capture(codex)?,
        })
    }
}

impl TaskUnits for SystemdTasks {
    fn launch(&mut self, attempt: &ResumeAttempt) -> Result<()> {
        control::execute(launch_arguments(
            &self.executable,
            &self.environment,
            attempt,
        )?)
    }

    fn inventory(&mut self, attempts: &[String]) -> Result<BTreeMap<String, TaskState>> {
        if attempts.is_empty() {
            return Ok(BTreeMap::new());
        }
        let requested = attempts
            .iter()
            .map(|attempt| Ok((unit_name(attempt)?, attempt.clone())))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut command = Command::new("systemctl");
        command.args(["--user", "show", "toks-router-resume-task-*.service"]);
        command.args(["--property=Id", "--property=LoadState"]);
        command.args(["--property=ActiveState", "--property=SubState"]);
        command.args(["--property=Result", "--property=ExecMainCode"]);
        command.arg("--property=ExecMainStatus");
        let output = control::bounded_output(command).context("querying resumed task units")?;
        let properties = String::from_utf8_lossy(&output.stdout);
        let (inventory, observed) = parse_inventory(&properties, &requested);
        anyhow::ensure!(
            output.status.success() || observed == requested.len(),
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(inventory)
    }

    fn cleanup(&mut self, attempt: &str) -> Result<()> {
        let unit = unit_name(attempt)?;
        let mut stop = Command::new("systemctl");
        stop.args(["--user", "stop", &unit]);
        control::checked_allow_not_found(stop, "stopping completed resumed task")?;
        let mut reset = Command::new("systemctl");
        reset.args(["--user", "reset-failed", &unit]);
        control::checked_allow_not_found(reset, "resetting completed resumed task")
    }

    fn cancel(&mut self, attempt: &str, state: TaskState) -> Result<()> {
        if state == TaskState::Missing {
            return Ok(());
        }
        let unit = unit_name(attempt)?;
        let mut stop = Command::new("systemctl");
        stop.args(["--user", "stop", &unit]);
        let output = control::bounded_output(stop).context("stopping cancelled resumed task")?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }
}

fn unit_name(attempt: &str) -> Result<String> {
    super::state::validate_attempt_id(attempt)?;
    Ok(format!("{UNIT_PREFIX}{attempt}.service"))
}

fn parse_state(properties: &str) -> TaskState {
    let value = |name: &str| properties.lines().find_map(|line| line.strip_prefix(name));
    if value("LoadState=") == Some("not-found") {
        return TaskState::Missing;
    }
    let natural_success = value("Result=") == Some("success")
        && matches!(value("ExecMainCode="), Some("1" | "exited"))
        && value("ExecMainStatus=") == Some("0");
    match (value("ActiveState="), value("SubState=")) {
        (Some("activating"), _) => TaskState::Starting,
        (Some("active"), Some("running")) => TaskState::Running,
        (Some("active"), Some("exited")) if natural_success => TaskState::Succeeded,
        (Some("failed"), _) => TaskState::Failed,
        (Some("inactive"), _) if natural_success => TaskState::Succeeded,
        _ => TaskState::Failed,
    }
}

fn parse_inventory(
    properties: &str,
    requested: &BTreeMap<String, String>,
) -> (BTreeMap<String, TaskState>, usize) {
    let mut inventory = requested
        .values()
        .map(|attempt| (attempt.clone(), TaskState::Missing))
        .collect::<BTreeMap<_, _>>();
    let mut observed = 0;
    for unit in properties.split("\n\n") {
        let id = unit.lines().find_map(|line| line.strip_prefix("Id="));
        if let Some(attempt) = id.and_then(|id| requested.get(id)) {
            inventory.insert(attempt.clone(), parse_state(unit));
            observed += 1;
        }
    }
    (inventory, observed)
}

#[cfg(test)]
mod tests;
