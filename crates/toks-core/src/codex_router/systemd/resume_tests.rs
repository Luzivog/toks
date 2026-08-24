use super::install_receipt::PendingInstall;
use super::plan::{install_plan, Action, InstallFacts};
use super::units::UnitEnvironment;
use crate::codex_router::host::BuildId;

fn facts() -> InstallFacts {
    InstallFacts {
        service_active: true,
        socket_active: true,
        resume_active: true,
        resume_matches: true,
        coordinator_matches: true,
        restart_coordinator: false,
        restart_resume: false,
    }
}

#[test]
fn coordinator_restart_never_restarts_the_resume_supervisor() {
    let actions = install_plan(InstallFacts {
        restart_coordinator: true,
        ..facts()
    });
    assert_eq!(
        actions,
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::RestartCoordinator,
        ]
    );
    assert!(!actions.contains(&Action::RestartResume));
}

#[test]
fn missing_resume_supervisor_starts_without_touching_the_coordinator() {
    assert_eq!(
        install_plan(InstallFacts {
            resume_active: false,
            resume_matches: false,
            ..facts()
        }),
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::StartResume,
        ]
    );
}

#[test]
fn resume_unit_change_restarts_only_the_resume_supervisor() {
    let mut pending = PendingInstall::default();
    pending.record_changes([false, false, false, true], false);

    assert!(pending.requires_action());
    assert_eq!(
        install_plan(InstallFacts {
            restart_resume: pending.restart_resume,
            ..facts()
        }),
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::RestartResume,
        ]
    );
}

#[test]
fn resume_supervisor_unit_has_an_exact_build_receipt_and_no_coordinator_binding() {
    let environment = UnitEnvironment::from_values([Some("/bin"), None, None, None]);
    let unit = super::resume_unit::render(
        std::path::Path::new("/opt/toks-router"),
        std::path::Path::new("/opt/codex"),
        &BuildId::new("candidate").unwrap(),
        &environment,
    )
    .unwrap();
    assert!(unit.contains("ExecStart=\"/opt/toks-router\" launch-resume-supervisor"));
    assert!(unit.contains("Restart=always"));
    assert!(unit.contains("TOKS_CODEX_BIN=/opt/codex"));
    assert!(unit.contains("TOKS_ROUTER_BUILD_ID=candidate"));
    assert!(!unit.contains("PartOf=toks-router.service"));
    assert!(!unit.contains("BindsTo=toks-router.service"));
}
