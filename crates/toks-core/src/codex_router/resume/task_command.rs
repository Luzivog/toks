use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::rotation::ThreadId;

const CONTINUE_PROMPT: &str =
    "Continue the interrupted task from where it stopped. Do not repeat completed work.";

pub(super) async fn run_codex(
    attempt: &str,
    thread: ThreadId,
    cwd: PathBuf,
) -> Result<std::process::ExitStatus> {
    let executable = crate::codex_router::codex_binary::discover()?;
    let mut command = resume_command(&executable, attempt, &thread, &cwd);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    execute(command).await
}

pub(super) async fn execute(command: Command) -> Result<std::process::ExitStatus> {
    tokio::process::Command::from(command)
        .status()
        .await
        .context("spawning Codex resume process")
}

fn resume_command(executable: &Path, attempt: &str, thread: &ThreadId, cwd: &Path) -> Command {
    let mut environment = crate::codex_router::systemd::allowed_environment();
    environment.insert("TOKS_RESUME_ATTEMPT".into(), attempt.into());
    let mut command = Command::new(executable);
    command
        .env_clear()
        .envs(environment)
        .args(["-c", "model_provider=\"toks_resume\""])
        .args([
            "-c",
            "model_providers.toks_resume.name=\"Toks resume\"",
        ])
        .args([
            "-c",
            "model_providers.toks_resume.base_url=\"http://127.0.0.1:47837/backend-api/codex\"",
        ])
        .args(["-c", "model_providers.toks_resume.wire_api=\"responses\""])
        .args(["-c", "model_providers.toks_resume.requires_openai_auth=true"])
        .args(["-c", "model_providers.toks_resume.supports_websockets=true"])
        .args([
            "-c",
            "model_providers.toks_resume.supports_standalone_web_search=true",
        ])
        .args([
            "-c",
            "model_providers.toks_resume.env_http_headers={\"x-toks-resume-attempt\"=\"TOKS_RESUME_ATTEMPT\"}",
        ])
        .args(["exec", "--skip-git-repo-check", "-C"])
        .arg(cwd)
        .args(["resume", "--all"])
        .arg(thread.as_str())
        .arg(CONTINUE_PROMPT);
    command
}

#[cfg(test)]
pub(super) fn command_for_test(
    executable: &Path,
    attempt: &str,
    thread: &ThreadId,
    cwd: &Path,
) -> Command {
    resume_command(executable, attempt, thread, cwd)
}

#[cfg(test)]
pub(super) const PROMPT_FOR_TEST: &str = CONTINUE_PROMPT;
