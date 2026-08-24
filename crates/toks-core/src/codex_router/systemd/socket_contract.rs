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

fn ensure_observed_candidate(active: bool, output: &str) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::ensure_candidate;

    const CANDIDATE: &str = "Listen=127.0.0.1:47837 (Stream)\nFileDescriptorName=router\nAccept=no\nBacklog=256\nNoDelay=yes\nSocketMode=0600\nReusePort=no\nFreeBind=no\nFlushPending=no\nKeepAlive=no\nPassCredentials=no\nPassSecurity=no\nPassPacketInfo=no\nTimestamping=off\nRemoveOnStop=no\nTriggers=toks-router.service\n";

    #[test]
    fn loaded_socket_contract_requires_the_exact_listener_and_semantics() {
        ensure_candidate(CANDIDATE).unwrap();
        for incompatible in [
            CANDIDATE.replace("127.0.0.1:47837 (Stream)", "127.0.0.1:47838 (Stream)"),
            CANDIDATE.replace("(Stream)", "(Datagram)"),
            CANDIDATE.replace("Accept=no", "Accept=yes"),
            CANDIDATE.replace("Backlog=256", "Backlog=128"),
            CANDIDATE.replace("NoDelay=yes", "NoDelay=no"),
            CANDIDATE.replace("FileDescriptorName=router", "FileDescriptorName=legacy"),
            CANDIDATE.replace("ReusePort=no", "ReusePort=yes"),
            CANDIDATE.replace("Triggers=toks-router.service", "Triggers=legacy.service"),
        ] {
            assert!(ensure_candidate(&incompatible).is_err());
        }
    }

    #[test]
    fn readiness_rejects_post_action_socket_deactivation_and_loaded_drift() {
        assert!(super::ensure_observed_candidate(false, CANDIDATE).is_err());
        let drifted = CANDIDATE.replace("NoDelay=yes", "NoDelay=no");
        assert!(super::ensure_observed_candidate(true, &drifted).is_err());
        super::ensure_observed_candidate(true, CANDIDATE).unwrap();
    }

    #[test]
    fn crash_replay_rejects_the_old_loaded_socket_before_coordinator_action() {
        struct FakeSystemd {
            active: bool,
            loaded: String,
            fragment: String,
            coordinator_touches: usize,
        }
        impl FakeSystemd {
            fn guarded_coordinator_action(&mut self) -> anyhow::Result<()> {
                anyhow::ensure!(self.active, "socket is inactive");
                super::ensure_candidate(&self.loaded)?;
                self.coordinator_touches += 1;
                Ok(())
            }
        }

        let mut systemd = FakeSystemd {
            active: false,
            loaded: CANDIDATE.replace("Backlog=256", "Backlog=64"),
            fragment: systemd_old_fragment(),
            coordinator_touches: 0,
        };
        assert!(!systemd.active); // First installer sample.
        systemd.active = true; // The manager activates its old loaded unit.
        systemd.fragment = CANDIDATE.into(); // Installer writes the new fragment, then crashes.

        let replay = systemd.guarded_coordinator_action();

        assert!(replay.is_err());
        assert_eq!(systemd.coordinator_touches, 0);
        assert!(super::ensure_candidate(&systemd.fragment).is_ok());
    }

    fn systemd_old_fragment() -> String {
        CANDIDATE.replace("Backlog=256", "Backlog=64")
    }
}
