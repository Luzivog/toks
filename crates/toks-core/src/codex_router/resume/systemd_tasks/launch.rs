use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

use super::super::state::ResumeAttempt;
use super::unit_name;

#[derive(Debug)]
pub(super) struct TaskEnvironment(BTreeMap<String, String>);

impl TaskEnvironment {
    pub(super) fn capture(codex: PathBuf) -> Result<Self> {
        let build = std::env::var("TOKS_ROUTER_BUILD_ID")
            .context("missing TOKS_ROUTER_BUILD_ID in resume supervisor")?;
        Self::from_unit(
            codex,
            build,
            crate::codex_router::systemd::UnitEnvironment::capture(),
        )
    }

    pub(super) fn from_unit(
        codex: PathBuf,
        build: String,
        captured: crate::codex_router::systemd::UnitEnvironment,
    ) -> Result<Self> {
        let mut values = captured
            .values()
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name, value)))
            .collect::<BTreeMap<_, _>>();
        values.insert(
            "TOKS_CODEX_BIN".into(),
            utf8(&codex, "Codex executable")?.into(),
        );
        values.insert("TOKS_ROUTER_BUILD_ID".into(), build);
        Ok(Self(values))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskLaunch {
    executable: String,
    environment: BTreeMap<String, String>,
    attempt: String,
    thread: String,
    cwd: String,
}

pub(super) fn launch_arguments(
    executable: &Path,
    environment: &TaskEnvironment,
    attempt: &ResumeAttempt,
) -> Result<Vec<OsString>> {
    let launch = TaskLaunch {
        executable: utf8(executable, "router executable")?.into(),
        environment: environment.0.clone(),
        attempt: attempt.id.clone(),
        thread: attempt.waiting.thread_id.as_str().into(),
        cwd: utf8(&attempt.cwd, "resume workspace")?.into(),
    };
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&launch)?);
    Ok(vec![
        "--unit".into(),
        unit_name(&attempt.id)?.into(),
        "--property=RemainAfterExit=yes".into(),
        "--property=CollectMode=inactive".into(),
        "--property=KillMode=control-group".into(),
        "--property=TimeoutStopSec=15s".into(),
        "--".into(),
        format!("/proc/{}/exe", std::process::id()).into(),
        "launch-resume-task".into(),
        encoded.into(),
    ])
}

pub(in crate::codex_router) fn execute(encoded: &str) -> Result<()> {
    let launch = decode(encoded)?;
    let expected = Path::new(&launch.executable)
        .canonicalize()
        .context("canonicalizing resume-task executable")?;
    anyhow::ensure!(
        expected == std::env::current_exe()?.canonicalize()?,
        "resume-task executable does not match the supervisor"
    );
    let mut command = command(launch)?;
    Err(command.exec()).context("launching exact-environment resume task")
}

fn command(launch: TaskLaunch) -> Result<std::process::Command> {
    super::super::state::validate_attempt_id(&launch.attempt)?;
    crate::codex_router::systemd::validate_allowed_environment(&launch.environment)?;
    Ok(crate::codex_router::systemd::exact_command(
        Path::new(&launch.executable),
        [
            "resume-task",
            launch.attempt.as_str(),
            launch.thread.as_str(),
            launch.cwd.as_str(),
        ],
        &launch.environment,
    ))
}

fn decode(encoded: &str) -> Result<TaskLaunch> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .context("decoding resume-task launch")?;
    serde_json::from_slice(&bytes).context("parsing resume-task launch")
}

fn utf8<'a>(path: &'a Path, label: &str) -> Result<&'a str> {
    path.to_str()
        .with_context(|| format!("{label} is not UTF-8"))
}

#[cfg(test)]
pub(super) fn command_for_test(encoded: &str) -> Result<std::process::Command> {
    command(decode(encoded)?)
}
