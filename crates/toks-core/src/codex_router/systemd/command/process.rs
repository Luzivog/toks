use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

pub(in crate::codex_router::systemd) fn coordinator_matches_until(
    executable: &Path,
    environment: &BTreeMap<String, Option<String>>,
    deadline: Instant,
) -> Result<bool> {
    matches_until(
        "toks-router.service",
        executable,
        b"host",
        environment,
        true,
        deadline,
    )
}

pub(in crate::codex_router) fn coordinator_main_pid_until(
    deadline: Instant,
) -> Result<Option<i32>> {
    Ok(super::systemctl_stdout_until(
        &[
            "show",
            "--property=MainPID",
            "--value",
            "toks-router.service",
        ],
        deadline,
    )?
    .trim()
    .parse::<i32>()
    .ok()
    .filter(|pid| *pid > 0))
}

pub(in crate::codex_router::systemd) fn resume_matches_until(
    executable: &Path,
    environment: &BTreeMap<String, Option<String>>,
    deadline: Instant,
) -> Result<bool> {
    matches_until(
        "toks-router-resume.service",
        executable,
        b"resume-supervisor",
        environment,
        false,
        deadline,
    )
}

fn matches_until(
    unit: &str,
    executable: &Path,
    subcommand: &[u8],
    environment: &BTreeMap<String, Option<String>>,
    socket_activated: bool,
    deadline: Instant,
) -> Result<bool> {
    let Some(pid) =
        super::systemctl_stdout_until(&["show", "--property=MainPID", "--value", unit], deadline)?
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|pid| *pid != 0)
    else {
        return Ok(false);
    };
    let process = PathBuf::from(format!("/proc/{pid}"));
    let mut environment = environment.clone();
    if socket_activated {
        environment
            .extend(crate::codex_router::systemd::runtime_process::activation_environment(pid));
    }
    Ok(process_matches(
        &process,
        executable,
        subcommand,
        &environment,
    ))
}

pub(super) fn process_matches(
    process: &Path,
    executable: &Path,
    subcommand: &[u8],
    expected_environment: &BTreeMap<String, Option<String>>,
) -> bool {
    let same_executable = executable
        .canonicalize()
        .and_then(|expected| {
            process
                .join("exe")
                .canonicalize()
                .map(|found| expected == found)
        })
        .unwrap_or(false);
    let arguments = fs::read(process.join("cmdline")).unwrap_or_default();
    let mut arguments = arguments.split(|byte| *byte == 0).collect::<Vec<_>>();
    if arguments.last() == Some(&b"".as_slice()) {
        arguments.pop();
    }
    same_executable
        && arguments.len() == 2
        && arguments[1] == subcommand
        && environment_matches(
            &fs::read(process.join("environ")).unwrap_or_default(),
            expected_environment,
        )
}

fn environment_matches(environ: &[u8], expected: &BTreeMap<String, Option<String>>) -> bool {
    let entries = environ
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    let expected_count = expected.values().filter(|value| value.is_some()).count();
    entries.len() == expected_count
        && expected.iter().all(|(name, value)| {
            let prefix = format!("{name}=");
            let found = entries
                .iter()
                .filter(|entry| entry.starts_with(prefix.as_bytes()))
                .copied()
                .collect::<Vec<_>>();
            match value {
                Some(value) => {
                    let expected = format!("{name}={value}");
                    found.len() == 1 && found[0] == expected.as_bytes()
                }
                None => found.is_empty(),
            }
        })
}
