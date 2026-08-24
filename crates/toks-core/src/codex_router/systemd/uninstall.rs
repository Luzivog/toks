use std::time::{Duration, Instant};

use anyhow::Result;

use super::plan::{uninstall_plan, Action};
use super::{
    execute_until, is_unit_active_until, named_unit_path, remove_if_exists, LifecycleGuard,
    PENDING_NAME, RESUME_NAME, SOCKET_NAME, UNIT_NAME, WORKER_NAME,
};

pub(super) const TIMEOUT: Duration = Duration::from_secs(75);

pub(super) fn run(_lifecycle: &LifecycleGuard) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    let mut failures = Vec::new();
    for action in uninstall_plan() {
        match action_is_relevant(action, deadline) {
            Ok(true) => record(&mut failures, action, execute_until(action, deadline)),
            Ok(false) => {}
            Err(error) => failures.push(format!("checking {action:?}: {error:#}")),
        }
    }
    for name in [
        UNIT_NAME,
        SOCKET_NAME,
        WORKER_NAME,
        RESUME_NAME,
        PENDING_NAME,
    ] {
        if let Err(error) = named_unit_path(name).and_then(|path| remove_if_exists(&path)) {
            failures.push(format!("removing {name}: {error:#}"));
        }
    }
    record(
        &mut failures,
        Action::DaemonReload,
        execute_until(Action::DaemonReload, deadline),
    );
    anyhow::ensure!(failures.is_empty(), "{}", failures.join("; "));
    Ok(())
}

fn action_is_relevant(action: Action, deadline: Instant) -> Result<bool> {
    Ok(match action {
        Action::DisableSocket => {
            named_unit_path(SOCKET_NAME)?.exists() || is_unit_active_until(SOCKET_NAME, deadline)?
        }
        Action::DisableCoordinator => {
            named_unit_path(UNIT_NAME)?.exists() || is_unit_active_until(UNIT_NAME, deadline)?
        }
        Action::DisableResume => {
            named_unit_path(RESUME_NAME)?.exists() || is_unit_active_until(RESUME_NAME, deadline)?
        }
        Action::StopWorkers => is_unit_active_until("toks-router-worker@*.service", deadline)?,
        _ => true,
    })
}

fn record(failures: &mut Vec<String>, action: Action, result: Result<()>) {
    if let Err(error) = result {
        failures.push(format!("running {action:?}: {error:#}"));
    }
}
