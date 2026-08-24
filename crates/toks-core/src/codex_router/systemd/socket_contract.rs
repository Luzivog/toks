use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Context, Result};

use super::{command, ROUTER_PORT, SOCKET_NAME};

const PROPERTIES: [(&str, &str); 16] = [
    ("Listen", "127.0.0.1:47837 (Stream)"),
    ("FileDescriptorName", "router"),
    ("Accept", "no"),
    ("Backlog", "256"),
    ("NoDelay", "yes"),
    ("SocketMode", "0600"),
    ("ReusePort", "no"),
    ("FreeBind", "no"),
    ("FlushPending", "no"),
    ("KeepAlive", "no"),
    ("PassCredentials", "no"),
    ("PassSecurity", "no"),
    ("PassPacketInfo", "no"),
    ("Timestamping", "off"),
    ("RemoveOnStop", "no"),
    ("Triggers", "toks-router.service"),
];
const QUERY: [&str; 18] = [
    "show",
    "--property=Listen",
    "--property=FileDescriptorName",
    "--property=Accept",
    "--property=Backlog",
    "--property=NoDelay",
    "--property=SocketMode",
    "--property=ReusePort",
    "--property=FreeBind",
    "--property=FlushPending",
    "--property=KeepAlive",
    "--property=PassCredentials",
    "--property=PassSecurity",
    "--property=PassPacketInfo",
    "--property=Timestamping",
    "--property=RemoveOnStop",
    "--property=Triggers",
    SOCKET_NAME,
];

pub(super) fn ensure_active_candidate_until(deadline: Instant) -> Result<()> {
    let active = command::is_unit_active_until(SOCKET_NAME, deadline)?;
    if !active {
        return ensure_observed_candidate(false, "");
    }
    let output = command::systemctl_stdout_until(&QUERY, deadline)?;
    ensure_observed_candidate(true, &output)
}

pub(super) fn ensure_observed_candidate(active: bool, output: &str) -> Result<()> {
    anyhow::ensure!(active, "toks-router.socket is not active");
    ensure_candidate(output)
}

pub(super) fn ensure_candidate(output: &str) -> Result<()> {
    let mut found = BTreeMap::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once('=')
            .context("systemd socket property has no value")?;
        anyhow::ensure!(
            found.insert(name, value).is_none(),
            "systemd repeated socket property {name}"
        );
    }
    for (name, expected) in PROPERTIES {
        let actual = found
            .get(name)
            .with_context(|| format!("systemd omitted socket property {name}"))?;
        anyhow::ensure!(
            *actual == expected,
            "active toks-router.socket has incompatible {name}: expected {expected}, found {actual}"
        );
    }
    anyhow::ensure!(
        ROUTER_PORT == 47_837,
        "socket contract port constant is inconsistent"
    );
    Ok(())
}
