use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use super::units::UnitEnvironment;

const REQUIRED_NAMES: [&str; 2] = ["TOKS_CODEX_BIN", "TOKS_ROUTER_BUILD_ID"];
const ACTIVATION_NAMES: [&str; 3] = ["LISTEN_PID", "LISTEN_FDS", "LISTEN_FDNAMES"];

pub(in crate::codex_router) fn allowed_environment() -> BTreeMap<String, String> {
    let mut environment = UnitEnvironment::capture()
        .values()
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
        .collect::<BTreeMap<_, _>>();
    for name in REQUIRED_NAMES {
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.into(), value);
        }
    }
    environment
}

pub(in crate::codex_router) fn exact_command(
    executable: &Path,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    environment: &BTreeMap<String, String>,
) -> Command {
    let mut command = Command::new(executable);
    command.args(arguments).env_clear().envs(environment);
    command
}

pub(super) fn launch_static(subcommand: &str, socket_activated: bool) -> Result<()> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let mut environment = allowed_environment();
    for name in REQUIRED_NAMES {
        anyhow::ensure!(environment.contains_key(name), "missing {name}");
    }
    if socket_activated {
        for name in ACTIVATION_NAMES {
            environment.insert(name.into(), std::env::var(name).with_context(|| name)?);
        }
    }
    let mut command = exact_command(&executable, [subcommand], &environment);
    Err(command.exec()).with_context(|| format!("launching router {subcommand}"))
}

pub(super) fn activation_environment(pid: u32) -> BTreeMap<String, Option<String>> {
    BTreeMap::from([
        ("LISTEN_PID".into(), Some(pid.to_string())),
        ("LISTEN_FDS".into(), Some("1".into())),
        ("LISTEN_FDNAMES".into(), Some("router".into())),
    ])
}

pub(in crate::codex_router) fn validate_allowed_environment(
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    let allowed = UnitEnvironment::names()
        .chain(REQUIRED_NAMES)
        .collect::<std::collections::BTreeSet<_>>();
    anyhow::ensure!(
        environment
            .keys()
            .all(|name| allowed.contains(name.as_str())),
        "resume task environment contains a non-allowlisted name"
    );
    for name in REQUIRED_NAMES {
        anyhow::ensure!(environment.contains_key(name), "missing {name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::exact_command;
    use std::collections::BTreeMap;

    #[test]
    fn exact_command_removes_the_real_ambient_environment() {
        let expected = BTreeMap::from([
            ("PATH".into(), "/usr/bin$literal%value".into()),
            ("TOKS_CODEX_BIN".into(), "/opt/codex$literal%value".into()),
        ]);
        let output = exact_command(
            std::path::Path::new("/usr/bin/env"),
            [] as [&str; 0],
            &expected,
        )
        .output()
        .unwrap();
        assert!(output.status.success());
        let output = String::from_utf8(output.stdout).unwrap();
        let actual = output
            .lines()
            .map(|line| line.split_once('=').unwrap())
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual.len(), 2);
        assert_eq!(actual["PATH"], "/usr/bin$literal%value");
        assert_eq!(actual["TOKS_CODEX_BIN"], "/opt/codex$literal%value");
        assert!(!actual.contains_key("OPENAI_API_KEY"));
        assert!(!actual.contains_key("LD_PRELOAD"));
    }
}
