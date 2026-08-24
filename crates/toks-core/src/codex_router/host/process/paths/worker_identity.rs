use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::codex_router::host::GenerationId;

pub(super) fn matches(
    generations: &Path,
    generation: GenerationId,
    pid: i32,
    proc_root: &Path,
) -> bool {
    if pid <= 0 {
        return false;
    }
    let contract = generations
        .join(generation.get().to_string())
        .join(crate::codex_router::systemd::launch_contract_name());
    let Ok(expected) = crate::codex_router::systemd::worker_process_contract(&contract) else {
        return false;
    };
    let process = proc_root.join(pid.to_string());
    executable_matches(&process, &expected.executable)
        && arguments_match(&process, generation)
        && environment_matches(&process, &expected.environment)
        && cgroup_matches(&process, generation)
}

fn executable_matches(process: &Path, expected: &Path) -> bool {
    expected
        .canonicalize()
        .and_then(|expected| {
            process
                .join("exe")
                .canonicalize()
                .map(|found| expected == found)
        })
        .unwrap_or(false)
}

fn arguments_match(process: &Path, generation: GenerationId) -> bool {
    let bytes = fs::read(process.join("cmdline")).unwrap_or_default();
    let arguments = nul_entries(&bytes);
    arguments.len() == 3
        && arguments[1] == b"worker"
        && arguments[2] == generation.get().to_string().as_bytes()
}

fn environment_matches(process: &Path, expected: &BTreeMap<String, Option<String>>) -> bool {
    let bytes = fs::read(process.join("environ")).unwrap_or_default();
    let entries = nul_entries(&bytes);
    let mut actual = BTreeMap::new();
    for entry in entries {
        let Some(index) = entry.iter().position(|byte| *byte == b'=') else {
            return false;
        };
        let Ok(name) = std::str::from_utf8(&entry[..index]) else {
            return false;
        };
        let Ok(value) = std::str::from_utf8(&entry[index + 1..]) else {
            return false;
        };
        if actual.insert(name, value).is_some() {
            return false;
        }
    }
    let expected = expected
        .iter()
        .filter_map(|(name, value)| value.as_deref().map(|value| (name.as_str(), value)))
        .collect::<BTreeMap<_, _>>();
    actual == expected
}

fn cgroup_matches(process: &Path, generation: GenerationId) -> bool {
    let expected = format!("toks-router-worker@{}.service", generation.get());
    fs::read_to_string(process.join("cgroup"))
        .ok()
        .is_some_and(|cgroups| {
            cgroups.lines().any(|line| {
                line.splitn(3, ':')
                    .nth(2)
                    .is_some_and(|path| path.split('/').any(|part| part == expected))
            })
        })
}

fn nul_entries(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect()
}
