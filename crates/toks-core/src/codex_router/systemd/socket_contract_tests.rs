use super::socket_contract::ensure_candidate;

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
    assert!(super::socket_contract::ensure_observed_candidate(false, CANDIDATE).is_err());
    let drifted = CANDIDATE.replace("NoDelay=yes", "NoDelay=no");
    assert!(super::socket_contract::ensure_observed_candidate(true, &drifted).is_err());
    super::socket_contract::ensure_observed_candidate(true, CANDIDATE).unwrap();
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
            super::socket_contract::ensure_candidate(&self.loaded)?;
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
    assert!(super::socket_contract::ensure_candidate(&systemd.fragment).is_ok());
}

fn systemd_old_fragment() -> String {
    CANDIDATE.replace("Backlog=256", "Backlog=64")
}
