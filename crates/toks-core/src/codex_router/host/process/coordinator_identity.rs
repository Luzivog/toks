use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::codex_router::handoff::PeerIdentity;
use crate::codex_router::host::BuildId;

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_millis(750);
const BUILD_ENVIRONMENT: &str = "TOKS_ROUTER_BUILD_ID";

pub(super) async fn authorize(peer: PeerIdentity, artifact_root: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || authorize_blocking(peer, &artifact_root))
        .await
        .unwrap_or(false)
}

fn authorize_blocking(peer: PeerIdentity, artifact_root: &Path) -> bool {
    if peer.uid != nix::unistd::Uid::current().as_raw() {
        return false;
    }
    let deadline = Instant::now() + AUTHORIZATION_TIMEOUT;
    let Ok(first) = crate::codex_router::systemd::coordinator_main_pid_until(deadline) else {
        return false;
    };
    if first != Some(peer.pid) {
        return false;
    }
    let matches = process_matches(peer.pid, artifact_root, Path::new("/proc"));
    let Ok(second) = crate::codex_router::systemd::coordinator_main_pid_until(deadline) else {
        return false;
    };
    matches && second == first
}

#[cfg(test)]
pub(super) fn matches_snapshot(
    peer: PeerIdentity,
    expected_uid: u32,
    first_main_pid: Option<i32>,
    second_main_pid: Option<i32>,
    artifact_root: &Path,
    proc_root: &Path,
) -> bool {
    peer.uid == expected_uid
        && first_main_pid == Some(peer.pid)
        && second_main_pid == first_main_pid
        && process_matches(peer.pid, artifact_root, proc_root)
}

fn process_matches(pid: i32, artifact_root: &Path, proc_root: &Path) -> bool {
    if pid <= 0 {
        return false;
    }
    let process = proc_root.join(pid.to_string());
    let Some(actual_environment) = read_environment(&process) else {
        return false;
    };
    let Some(build) = actual_environment
        .get(BUILD_ENVIRONMENT)
        .filter(|value| valid_build(value))
        .and_then(|value| BuildId::new(value.clone()).ok())
    else {
        return false;
    };
    let Ok(expected) =
        crate::codex_router::systemd::coordinator_process_contract(artifact_root, &build)
    else {
        return false;
    };
    executable_matches(&process, &expected.executable)
        && arguments_match(&process)
        && environment_matches(pid, &actual_environment, &expected.environment)
        && cgroup_matches(&process)
}

fn executable_matches(process: &Path, expected: &Path) -> bool {
    expected
        .canonicalize()
        .and_then(|expected| {
            process
                .join("exe")
                .canonicalize()
                .map(|found| found == expected)
        })
        .unwrap_or(false)
}

fn arguments_match(process: &Path) -> bool {
    let bytes = fs::read(process.join("cmdline")).unwrap_or_default();
    let arguments = nul_entries(&bytes);
    arguments.len() == 2 && arguments[1] == b"host"
}

fn read_environment(process: &Path) -> Option<BTreeMap<String, String>> {
    let bytes = fs::read(process.join("environ")).ok()?;
    let mut environment = BTreeMap::new();
    for entry in nul_entries(&bytes) {
        let split = entry.iter().position(|byte| *byte == b'=')?;
        let name = std::str::from_utf8(&entry[..split]).ok()?;
        let value = std::str::from_utf8(&entry[split + 1..]).ok()?;
        if environment
            .insert(name.to_owned(), value.to_owned())
            .is_some()
        {
            return None;
        }
    }
    Some(environment)
}

fn environment_matches(
    pid: i32,
    actual: &BTreeMap<String, String>,
    expected: &BTreeMap<String, Option<String>>,
) -> bool {
    let mut expected = expected
        .iter()
        .filter_map(|(name, value)| value.clone().map(|value| (name.clone(), value)))
        .collect::<BTreeMap<_, _>>();
    expected.extend([
        ("LISTEN_PID".into(), pid.to_string()),
        ("LISTEN_FDS".into(), "1".into()),
        ("LISTEN_FDNAMES".into(), "router".into()),
    ]);
    actual == &expected
}

fn cgroup_matches(process: &Path) -> bool {
    fs::read_to_string(process.join("cgroup"))
        .ok()
        .is_some_and(|cgroups| {
            cgroups.lines().any(|line| {
                line.splitn(3, ':')
                    .nth(2)
                    .is_some_and(|path| path.split('/').any(|part| part == "toks-router.service"))
            })
        })
}

fn valid_build(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn nul_entries(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect()
}
