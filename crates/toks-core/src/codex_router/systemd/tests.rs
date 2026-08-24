use std::fs;

use tempfile::tempdir;

use super::install_receipt::{self, PendingInstall};
use super::launch_contract::{command_for_test, inspect, LaunchContract, CONTRACT_NAME};
use super::plan::{install_plan, uninstall_plan, Action, InstallFacts};
use super::receipt::{active_candidate_generation, build_id, failed_candidate};
use super::units::{render_service_unit, render_worker_unit, UnitEnvironment};
use super::{persist_test_launch_contract, stage_generation};
use crate::codex_router::host::{BuildId, DeployPlan, DeploymentEvent, DeploymentState};

mod install_lock;
mod redeployment_receipt;
mod stable_artifacts;
mod unit_rendering;

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
fn legacy_bootstrap_stops_the_monolith_before_activating_the_socket() {
    let actions = install_plan(InstallFacts {
        socket_active: false,
        coordinator_matches: false,
        ..facts()
    });
    assert_eq!(
        actions,
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::StopCoordinator,
            Action::StartSocket,
            Action::StartCoordinator,
        ]
    );
}

#[test]
fn an_artifact_update_restarts_only_the_coordinator() {
    let actions = install_plan(InstallFacts {
        coordinator_matches: false,
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
}

#[test]
fn an_active_socket_is_never_restarted_by_installation() {
    let actions = install_plan(facts());
    assert_eq!(actions, [Action::DaemonReload, Action::EnableTopology]);
}

#[test]
fn an_exact_rerun_does_not_restart_any_process() {
    assert_eq!(
        install_plan(facts()),
        [Action::DaemonReload, Action::EnableTopology]
    );
}

#[test]
fn a_crash_after_first_socket_start_does_not_rebind_it_on_replay() {
    let after_socket_started = InstallFacts {
        service_active: false,
        ..facts()
    };
    assert_eq!(
        install_plan(after_socket_started),
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::StartCoordinator,
        ]
    );
}

#[test]
fn an_inactive_new_topology_starts_socket_before_coordinator() {
    assert_eq!(
        install_plan(InstallFacts {
            service_active: false,
            socket_active: false,
            resume_active: false,
            resume_matches: false,
            coordinator_matches: false,
            restart_coordinator: true,
            restart_resume: true,
        }),
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::StartSocket,
            Action::StartCoordinator,
            Action::StartResume,
        ]
    );
}

#[test]
fn an_existing_socket_recovers_a_missing_coordinator() {
    assert_eq!(
        install_plan(InstallFacts {
            service_active: false,
            coordinator_matches: false,
            ..facts()
        }),
        [
            Action::DaemonReload,
            Action::EnableTopology,
            Action::StartCoordinator,
        ]
    );
}

#[test]
fn uninstall_prevents_socket_reactivation_before_stopping_workers() {
    assert_eq!(
        uninstall_plan(),
        [
            Action::DisableResume,
            Action::DisableSocket,
            Action::DisableCoordinator,
            Action::StopWorkers,
        ]
    );
}

#[test]
fn uninstall_has_one_budget_for_every_stop_phase() {
    assert!(super::uninstall::TIMEOUT >= std::time::Duration::from_secs(75));
    assert_eq!(uninstall_plan().len(), 4);
}

#[test]
fn readiness_requires_the_candidate_build_to_be_active() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.json");
    let old = BuildId::new("old-build").unwrap();
    let candidate = BuildId::new("candidate-build").unwrap();
    let mut state = DeploymentState::default();
    let target = match state.plan_deploy(old.clone()).unwrap() {
        DeployPlan::StageTarget { target, .. } => target,
        plan => panic!("unexpected plan: {plan:?}"),
    };
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    assert_eq!(
        active_candidate_generation(&path, &old).unwrap(),
        Some(target.get())
    );
    assert_eq!(
        active_candidate_generation(&path, &candidate).unwrap(),
        None
    );
}

#[test]
fn readiness_rejects_a_structurally_readable_invalid_deployment() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.json");
    let build = BuildId::new("candidate-build").unwrap();
    let mut state = DeploymentState::default();
    activate(&mut state, build.clone());
    let mut value = serde_json::to_value(&state).unwrap();
    let generations = value["generations"].as_object_mut().unwrap();
    let mut duplicate = generations.values().next().unwrap().clone();
    duplicate["status"] = serde_json::json!("active");
    generations.insert("99".into(), duplicate);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

    let error = active_candidate_generation(&path, &build).unwrap_err();

    assert!(error
        .to_string()
        .contains("validating router deployment state"));
}

#[test]
fn deployment_identity_includes_the_artifact_and_worker_launch_contract() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("toks-router");
    fs::write(&path, b"candidate").unwrap();

    let old = UnitEnvironment::from_values([Some("OLD"), None, None, None]);
    let new = UnitEnvironment::from_values([Some("NEW"), None, None, None]);
    let original = LaunchContract::capture(&path, std::path::Path::new("/opt/codex"), &old)
        .unwrap()
        .build_id()
        .unwrap();
    assert_eq!(
        LaunchContract::capture(&path, std::path::Path::new("/opt/codex"), &old)
            .unwrap()
            .build_id()
            .unwrap(),
        original
    );
    assert_ne!(
        LaunchContract::capture(&path, std::path::Path::new("/opt/codex"), &new)
            .unwrap()
            .build_id()
            .unwrap(),
        original
    );
    fs::write(&path, b"different candidate").unwrap();
    assert_ne!(
        LaunchContract::capture(&path, std::path::Path::new("/opt/codex"), &old)
            .unwrap()
            .build_id()
            .unwrap(),
        original
    );
}

#[test]
fn staging_an_older_intent_uses_its_own_artifact_and_contract() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("router");
    let build_b_executable = executable(&root, "artifact-b", b"router-b");
    let build_c_executable = executable(&root, "artifact-c", b"router-c");
    let environment_b = UnitEnvironment::from_values([Some("/b/bin"), Some("/b/home"), None, None]);
    let environment_c = UnitEnvironment::from_values([Some("/c/bin"), Some("/c/home"), None, None]);
    let build_b = persist_test_launch_contract(
        &root,
        &build_b_executable,
        std::path::Path::new("/b/codex"),
        &environment_b,
    )
    .unwrap();
    persist_test_launch_contract(
        &root,
        &build_c_executable,
        std::path::Path::new("/c/codex"),
        &environment_c,
    )
    .unwrap();

    let generation = root.join("generations/7");
    stage_generation(&root, &generation, &build_b).unwrap();

    let staged = generation.join("toks-router").canonicalize().unwrap();
    assert!(staged.starts_with(root.join("executables")));
    assert_eq!(fs::read(&staged).unwrap(), b"router-b");
    let (found, executable, environment) = inspect(&generation.join(CONTRACT_NAME)).unwrap();
    assert_eq!(found, build_b);
    assert_eq!(executable, staged);
    assert_eq!(environment["TOKS_CODEX_BIN"].as_deref(), Some("/b/codex"));
    assert_eq!(environment["PATH"].as_deref(), Some("/b/bin"));
}

#[test]
fn old_generation_contract_survives_a_new_install_contract() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("router");
    let executable_a = executable(&root, "artifact-a", b"router-a");
    let executable_b = executable(&root, "artifact-b", b"router-b");
    let environment_a = UnitEnvironment::from_values([Some("/a/bin"), None, Some("/a/data"), None]);
    let environment_b = UnitEnvironment::from_values([Some("/b/bin"), Some("/b/home"), None, None]);
    let build_a = persist_test_launch_contract(
        &root,
        &executable_a,
        std::path::Path::new("/a/codex"),
        &environment_a,
    )
    .unwrap();
    let generation_a = root.join("generations/1");
    stage_generation(&root, &generation_a, &build_a).unwrap();

    persist_test_launch_contract(
        &root,
        &executable_b,
        std::path::Path::new("/b/codex"),
        &environment_b,
    )
    .unwrap();

    let (_, executable, environment) = inspect(&generation_a.join(CONTRACT_NAME)).unwrap();
    assert!(executable.starts_with(root.join("executables")));
    assert_eq!(fs::read(&executable).unwrap(), b"router-a");
    assert_eq!(environment["TOKS_CODEX_BIN"].as_deref(), Some("/a/codex"));
    assert_eq!(environment["PATH"].as_deref(), Some("/a/bin"));
    assert_eq!(environment["CODEX_HOME"], None);
    let command = command_for_test(&generation_a.join(CONTRACT_NAME), 1).unwrap();
    assert_eq!(command.get_program(), executable.as_os_str());
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == "TOKS_CODEX_BIN")
            .and_then(|(_, value)| value),
        Some(std::ffi::OsStr::new("/a/codex"))
    );
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == "CODEX_HOME")
            .and_then(|(_, value)| value),
        None
    );
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == "TOKS_ROUTER_BUILD_ID")
            .and_then(|(_, value)| value),
        Some(std::ffi::OsStr::new(build_a.as_str()))
    );
    let worker = render_worker_unit(&root).unwrap();
    assert!(!worker.contains("/b/codex"));
    assert!(worker.contains("generations/%i/launch.json"));
}

fn executable(root: &std::path::Path, directory: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = root.join(directory).join("toks-router");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn coordinator_uses_the_installer_contract_when_manager_environment_differs() {
    let directory = tempdir().unwrap();
    let executable = directory.path().join("toks-router");
    fs::write(&executable, b"candidate").unwrap();
    let codex = std::path::Path::new("/opt/codex");
    let installer =
        UnitEnvironment::from_values([Some("/installer/bin"), None, Some("/installer/data"), None]);
    let build = LaunchContract::capture(&executable, codex, &installer)
        .unwrap()
        .build_id()
        .unwrap();
    let coordinator = render_service_unit(&executable, codex, &build, &installer).unwrap();

    assert!(coordinator.contains(&format!("TOKS_ROUTER_BUILD_ID={}", build.as_str())));
    assert!(coordinator.contains("Environment=\"PATH=/installer/bin\""));
    assert!(coordinator.contains("UnsetEnvironment=CODEX_HOME"));
    assert!(coordinator.contains("Environment=\"XDG_DATA_HOME=/installer/data\""));
    assert!(coordinator.contains("UnsetEnvironment=XDG_CONFIG_HOME"));
    assert!(!coordinator.contains("manager"));
}

#[test]
fn worker_unit_change_requires_a_coordinator_restart() {
    let mut pending = PendingInstall::default();

    pending.record_changes([false, false, true, false], false);

    assert!(pending.restart_coordinator);
}

#[test]
fn socket_unit_change_never_schedules_a_process_restart() {
    let mut pending = PendingInstall::default();

    pending.record_changes([false, true, false, false], false);

    assert!(!pending.requires_action());
    assert_eq!(
        install_plan(facts()),
        [Action::DaemonReload, Action::EnableTopology]
    );
}

#[test]
fn changing_the_codex_launch_path_rotates_the_deployment_identity() {
    let directory = tempdir().unwrap();
    let executable = directory.path().join("toks-router");
    let stable = directory.path().join("router-artifacts");
    fs::write(&executable, b"same candidate").unwrap();

    let old = build_id(&stable, &executable, std::path::Path::new("/opt/codex-old")).unwrap();
    let new = build_id(&stable, &executable, std::path::Path::new("/opt/codex-new")).unwrap();

    assert_ne!(old, new);
}

#[test]
fn failed_candidate_is_retryable_but_an_active_candidate_is_not() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.json");
    let build = BuildId::new("candidate").unwrap();
    let mut failed_state = DeploymentState::default();
    let failed = match failed_state.plan_deploy(build.clone()).unwrap() {
        DeployPlan::StageTarget { target, .. } => target,
        plan => panic!("unexpected plan: {plan:?}"),
    };
    failed_state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "failed".into(),
        })
        .unwrap();
    fs::write(&path, serde_json::to_vec(&failed_state).unwrap()).unwrap();
    assert!(failed_candidate(&path, &build).unwrap());

    let mut active_state = DeploymentState::default();
    activate(&mut active_state, build.clone());
    fs::write(&path, serde_json::to_vec(&active_state).unwrap()).unwrap();
    assert!(!failed_candidate(&path, &build).unwrap());
}

fn activate(
    state: &mut DeploymentState,
    build: BuildId,
) -> crate::codex_router::host::GenerationId {
    let target = match state.plan_deploy(build).unwrap() {
        DeployPlan::StageTarget { target, .. } => target,
        plan => panic!("unexpected plan: {plan:?}"),
    };
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
        DeploymentEvent::TargetAccepting { target },
    ] {
        state.reconcile(event).unwrap();
    }
    target
}

#[test]
fn a_corrupt_pending_receipt_forces_full_convergence() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("pending.json");
    fs::write(&path, b"not-json").unwrap();

    let pending = install_receipt::load(&path);

    assert!(pending.restart_coordinator);
}

#[test]
fn completed_install_phases_are_removed_from_the_durable_receipt() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("pending.json");
    let mut pending = PendingInstall {
        restart_coordinator: true,
        restart_resume: true,
    };
    install_receipt::save(&path, &pending).unwrap();

    assert!(pending.completed(Action::StartCoordinator));
    install_receipt::save(&path, &pending).unwrap();
    assert!(install_receipt::load(&path).requires_action());
    assert!(pending.completed(Action::RestartResume));
    install_receipt::save(&path, &pending).unwrap();
    assert!(!install_receipt::load(&path).requires_action());
}
