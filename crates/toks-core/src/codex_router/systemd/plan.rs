#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    DaemonReload,
    EnableTopology,
    StopCoordinator,
    StartCoordinator,
    RestartCoordinator,
    StartSocket,
    StartResume,
    RestartResume,
    DisableSocket,
    DisableCoordinator,
    DisableResume,
    StopWorkers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InstallFacts {
    pub(super) service_active: bool,
    pub(super) socket_active: bool,
    pub(super) resume_active: bool,
    pub(super) resume_matches: bool,
    pub(super) coordinator_matches: bool,
    pub(super) restart_coordinator: bool,
    pub(super) restart_resume: bool,
}

pub(super) fn install_plan(facts: InstallFacts) -> Vec<Action> {
    let mut actions = vec![Action::DaemonReload, Action::EnableTopology];
    if !facts.socket_active {
        if facts.service_active {
            actions.push(Action::StopCoordinator);
        }
        actions.extend([Action::StartSocket, Action::StartCoordinator]);
    } else if !facts.service_active {
        actions.push(Action::StartCoordinator);
    } else if facts.restart_coordinator || !facts.coordinator_matches {
        actions.push(Action::RestartCoordinator);
    }
    if !facts.resume_active {
        actions.push(Action::StartResume);
    } else if facts.restart_resume || !facts.resume_matches {
        actions.push(Action::RestartResume);
    }
    actions
}

pub(super) fn uninstall_plan() -> [Action; 4] {
    [
        Action::DisableResume,
        Action::DisableSocket,
        Action::DisableCoordinator,
        Action::StopWorkers,
    ]
}

impl Action {
    pub(super) fn systemctl_args(self) -> &'static [&'static str] {
        match self {
            Self::DaemonReload => &["daemon-reload"],
            Self::EnableTopology => &[
                "enable",
                "toks-router.socket",
                "toks-router.service",
                "toks-router-resume.service",
            ],
            Self::StopCoordinator => &["stop", "toks-router.service"],
            Self::StartCoordinator => &["start", "toks-router.service"],
            Self::RestartCoordinator => &["restart", "toks-router.service"],
            Self::StartSocket => &["start", "toks-router.socket"],
            Self::StartResume => &["start", "toks-router-resume.service"],
            Self::RestartResume => &["restart", "toks-router-resume.service"],
            Self::DisableSocket => &["disable", "--now", "toks-router.socket"],
            Self::DisableCoordinator => &["disable", "--now", "toks-router.service"],
            Self::DisableResume => &["disable", "--now", "toks-router-resume.service"],
            Self::StopWorkers => &["stop", "toks-router-worker@*.service"],
        }
    }
}
