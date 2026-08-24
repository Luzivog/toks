use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::codex_router::host::BuildId;

use super::ROUTER_PORT;

const INHERITED_NAMES: [&str; 22] = [
    "PATH",
    "CODEX_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
    "XDG_RUNTIME_DIR",
    "HOME",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "LD_LIBRARY_PATH",
    "NIX_LD",
    "NIX_LD_LIBRARY_PATH",
];

const COORDINATOR_STOP_MARGIN_SECONDS: u64 = 13;
pub(in crate::codex_router) const COORDINATOR_STOP_TIMEOUT_SECONDS: u64 =
    crate::codex_router::host::COORDINATOR_PRE_SIGNAL_OPERATION_TIMEOUT.as_secs()
        + crate::codex_router::host::COORDINATOR_SHUTDOWN_DRAIN_TIMEOUT.as_secs()
        + COORDINATOR_STOP_MARGIN_SECONDS;

#[derive(Clone, Debug)]
pub(crate) struct UnitEnvironment {
    inherited: Vec<(&'static str, Option<String>)>,
}

impl UnitEnvironment {
    pub(super) fn names() -> impl Iterator<Item = &'static str> {
        INHERITED_NAMES.into_iter()
    }

    pub(in crate::codex_router) fn capture() -> Self {
        Self {
            inherited: INHERITED_NAMES
                .into_iter()
                .map(|name| (name, safe_value(name, std::env::var(name).ok())))
                .collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_values(values: [Option<&str>; 4]) -> Self {
        Self::from_pairs(
            &INHERITED_NAMES[..4]
                .iter()
                .copied()
                .zip(values)
                .collect::<Vec<_>>(),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_pairs(values: &[(&str, Option<&str>)]) -> Self {
        Self {
            inherited: INHERITED_NAMES
                .into_iter()
                .map(|name| {
                    let value = values
                        .iter()
                        .find_map(|(found, value)| (*found == name).then_some(*value))
                        .flatten()
                        .map(str::to_owned);
                    (name, safe_value(name, value))
                })
                .collect(),
        }
    }

    pub(super) fn directives(&self) -> Result<String> {
        self.inherited
            .iter()
            .map(|(name, value)| match value {
                Some(value) => Ok(format!("\nEnvironment={}", environment(name, value)?)),
                None => Ok(format!("\nUnsetEnvironment={name}")),
            })
            .collect()
    }

    pub(in crate::codex_router) fn values(&self) -> BTreeMap<String, Option<String>> {
        self.inherited
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect()
    }
}

fn safe_value(name: &str, value: Option<String>) -> Option<String> {
    let value = value?;
    let lower = name.to_ascii_lowercase();
    let is_proxy = lower.ends_with("_proxy") && lower != "no_proxy";
    // Proxy userinfo and query tokens must never enter a persisted launch contract.
    let contains_credentials = value
        .chars()
        .any(|character| matches!(character, '@' | '?' | '#'));
    (!is_proxy || !contains_credentials).then_some(value)
}

/// Renders the coordinator unit.
///
/// Deliberately carries no `ProtectSystem=`: any value puts the unit in a mount
/// namespace, and a namespaced process cannot read `/proc/<pid>/environ` or
/// `/proc/<pid>/exe` for any other process — the reads the coordinator and its
/// workers use to attest each other against their launch contracts. With it
/// set, activation times out forever and no worker is ever deployed.
/// `ProtectProc=` and `ProcSubset=` do not restore the access. The peer is
/// still constrained by `SocketMode=0600` on the control socket inside a `0700`
/// runtime directory plus `SO_PEERCRED` uid checks.
pub(crate) fn render_service_unit(
    executable: &Path,
    codex_executable: &Path,
    build: &BuildId,
    unit_environment: &UnitEnvironment,
) -> Result<String> {
    let executable = quote(executable)?;
    let codex = codex_executable
        .to_str()
        .context("Codex executable path is not UTF-8")?;
    let codex_environment = environment("TOKS_CODEX_BIN", codex)?;
    let build_environment = environment("TOKS_ROUTER_BUILD_ID", build.as_str())?;
    let inherited_environment = unit_environment.directives()?;
    Ok(format!(
        "[Unit]\nDescription=Toks Codex router coordinator\nAfter=network-online.target toks-router.socket\nWants=network-online.target\nRequires=toks-router.socket\n\n[Service]\nType=simple\nExecStart={executable} launch-host\nSockets=toks-router.socket\nEnvironment={codex_environment}\nEnvironment={build_environment}{inherited_environment}\nRestart=always\nRestartSec=2\nKillMode=control-group\nTimeoutStopSec={COORDINATOR_STOP_TIMEOUT_SECONDS}s\nRuntimeDirectory=toks-router\nRuntimeDirectoryMode=0700\nRuntimeDirectoryPreserve=restart\nUMask=0077\nNoNewPrivileges=true\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nLockPersonality=true\n\n[Install]\nWantedBy=default.target\n"
    ))
}

pub(crate) fn render_socket_unit() -> String {
    format!(
        "[Unit]\nDescription=Toks Codex router listener\n\n[Socket]\nListenStream=127.0.0.1:{ROUTER_PORT}\nFileDescriptorName=router\nSocketMode=0600\nAccept=no\nNoDelay=true\nBacklog=256\nReusePort=no\nFreeBind=no\nFlushPending=no\nKeepAlive=no\nPassCredentials=no\nPassSecurity=no\nPassPacketInfo=no\nTimestamping=off\nRemoveOnStop=no\n\n[Install]\nWantedBy=sockets.target\n"
    )
}

/// Renders the worker template unit.
///
/// Carries no `ProtectSystem=` for the same reason as the coordinator: a
/// mount-namespaced worker cannot read the coordinator's `/proc` entries and so
/// can never authorize it, leaving the handoff channel permanently unusable.
pub(crate) fn render_worker_unit(artifact_root: &Path) -> Result<String> {
    let generations = escaped_path(&artifact_root.join("generations"))?;
    let executable = format!("\"{generations}/%i/toks-router\"");
    let contract = format!(
        "\"{generations}/%i/{}\"",
        super::launch_contract::CONTRACT_NAME
    );
    Ok(format!(
        "[Unit]\nDescription=Toks Codex router worker %i\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={executable} launch-worker %i {contract}\nRestart=on-failure\nRestartSec=2\nTimeoutStopSec=15s\nUMask=0077\nNoNewPrivileges=true\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nLockPersonality=true\n"
    ))
}

pub(super) fn environment(name: &str, value: &str) -> Result<String> {
    reject_controls(name)?;
    reject_controls(value)?;
    let escaped = value
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    Ok(format!("\"{name}={escaped}\""))
}

pub(super) fn quote(path: &Path) -> Result<String> {
    Ok(format!("\"{}\"", escaped_path(path)?))
}

fn escaped_path(path: &Path) -> Result<String> {
    let value = path.to_str().context("systemd unit path is not UTF-8")?;
    reject_controls(value)?;
    Ok(value
        .replace('%', "%%")
        .replace('$', "$$")
        .replace('\\', "\\\\")
        .replace('"', "\\\""))
}

fn reject_controls(value: &str) -> Result<()> {
    anyhow::ensure!(
        !value.chars().any(char::is_control),
        "systemd unit value contains a control character"
    );
    Ok(())
}
