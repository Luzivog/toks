use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::accounts::CodexAuthSnapshot;
use crate::limits::Provider;

use super::catalogue::ModelChoice;
use super::model::{FailureReason, Launch, TASK_TIMEOUT_MS};

const PROMPT: &str = "test";

pub(super) async fn run(launch: &Launch) -> Result<(), FailureReason> {
    let config = exact_profile(launch).ok_or(FailureReason::ProfileUnavailable)?;
    let model = super::catalogue::best_for_profile(&config);
    let home = dirs::home_dir().ok_or(FailureReason::ProfileUnavailable)?;
    let executable =
        super::super::codex_binary::discover().map_err(|_| FailureReason::SpawnFailed)?;
    let command = command(&executable, &home, &config, &model);
    run_bounded(command).await
}

fn exact_profile(launch: &Launch) -> Option<PathBuf> {
    crate::accounts::discover_profiles()
        .into_iter()
        .find(|profile| {
            profile.provider == Provider::Codex
                && profile.profile_id == launch.profile_id
                && profile.account.id == launch.account
                && CodexAuthSnapshot::read(profile)
                    .is_ok_and(|auth| auth.account_id == launch.account)
        })
        .map(|profile| profile.config_dir)
}

fn command(executable: &Path, home: &Path, config: &Path, model: &ModelChoice) -> Command {
    let mut environment = super::super::systemd::allowed_environment();
    environment.insert("CODEX_HOME".into(), config.to_string_lossy().into_owned());
    environment.insert("HOME".into(), home.to_string_lossy().into_owned());
    // GNU timeout remains the task owner if the Toks process is killed, so the
    // Codex child cannot outlive the three-minute activation bound.
    let mut command = Command::new("timeout");
    command
        .env_clear()
        .envs(environment)
        .args(["--signal=TERM", "--kill-after=1s", "178s"])
        .arg(executable)
        .args(["exec", "--ignore-user-config", "--ignore-rules"])
        .arg("--skip-git-repo-check")
        .args(["-s", "read-only", "-C"])
        .arg(home)
        .args(["-c", "approval_policy=\"never\""])
        .args(["-c", "service_tier=\"default\""])
        .arg("-c")
        .arg(format!("model_reasoning_effort=\"{}\"", model.reasoning))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(slug) = &model.slug {
        command.arg("-m").arg(slug);
    }
    command.arg(PROMPT);
    command
}

async fn run_bounded(command: Command) -> Result<(), FailureReason> {
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| FailureReason::SpawnFailed)?;
    let timeout = Duration::from_millis(u64::try_from(TASK_TIMEOUT_MS).expect("positive timeout"));
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) if matches!(status.code(), Some(124 | 137)) => Err(FailureReason::TimedOut),
        Ok(Ok(_)) => Err(FailureReason::Unsuccessful),
        Ok(Err(_)) => Err(FailureReason::Unsuccessful),
        Err(_) => {
            let _ = child.kill().await;
            Err(FailureReason::TimedOut)
        }
    }
}

#[cfg(test)]
pub(super) fn command_for_test(
    executable: &Path,
    home: &Path,
    config: &Path,
    model: &ModelChoice,
) -> Command {
    command(executable, home, config, model)
}

#[cfg(test)]
pub(super) const PROMPT_FOR_TEST: &str = PROMPT;
