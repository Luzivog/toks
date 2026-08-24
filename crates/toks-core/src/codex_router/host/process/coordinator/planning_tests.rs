use futures_util::FutureExt;
use std::sync::{Arc, Mutex};

use super::super::channel::AsyncChannel;
use super::super::paths::HostPaths;
use super::core::{Coordinator, WorkerSlot};
use crate::codex_router::handoff::{
    Control, HandoffChannel, HandoffListener, Received, WorkerInstanceId,
};
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId,
};

#[derive(Clone, Copy, Debug)]
enum PriorPhase {
    StageTarget,
    Prepared,
    PreviousPaused,
    FailedRollback,
}

#[tokio::test]
async fn prior_build_phases_settle_before_the_current_build_is_staged() {
    for phase in [
        PriorPhase::StageTarget,
        PriorPhase::Prepared,
        PriorPhase::PreviousPaused,
        PriorPhase::FailedRollback,
    ] {
        let (directory, mut coordinator, previous, target, prior) = fixture(phase).await;
        assert_eq!(
            build_attempts(&coordinator, &coordinator.build),
            0,
            "{phase:?}"
        );
        assert_prior_plan(&coordinator.current_plan().unwrap(), phase, target, &prior);

        let calls = record_commands(&mut coordinator);
        coordinator.advance().await.unwrap();
        if matches!(phase, PriorPhase::StageTarget) {
            let staged = directory
                .path()
                .join("generations")
                .join(target.get().to_string());
            let executable = staged.join("toks-router").canonicalize().unwrap();
            assert!(executable.starts_with(directory.path().join("executables")));
            assert_eq!(
                std::fs::read(executable).unwrap(),
                std::fs::read(directory.path().join("prior/toks-router")).unwrap()
            );
            let contract = std::fs::read_to_string(staged.join("launch.json")).unwrap();
            assert!(contract.contains("/prior/codex"));
            assert!(!contract.contains("current-build-b"));
        }
        let expected = match phase {
            PriorPhase::FailedRollback => vec![("stop", target), ("start", previous)],
            _ => vec![("start", previous), ("start", target)],
        };
        assert_eq!(*calls.lock().unwrap(), expected, "{phase:?}");

        settle_prior(&mut coordinator, phase, target);
        let DeployPlan::StageTarget { build, .. } = coordinator.plan_for_advance().unwrap() else {
            panic!("current build was not staged after {phase:?} settled")
        };
        assert_eq!(build, coordinator.build);
        assert_eq!(build_attempts(&coordinator, &coordinator.build), 1);
    }
}

#[tokio::test]
async fn terminal_prior_build_states_stage_the_current_build_immediately() {
    for state in [terminal_accepting(), failed_before_pause(), unavailable()] {
        let (_directory, coordinator, _previous, _target, _prior) = from_state(state).await;
        assert!(matches!(
            coordinator.current_plan().unwrap(),
            DeployPlan::StageTarget { ref build, .. } if build == &coordinator.build
        ));
        assert_eq!(build_attempts(&coordinator, &coordinator.build), 1);
    }
}

#[tokio::test]
async fn retirement_finishes_before_the_current_build_is_staged() {
    let (state, previous, _target, _build) = terminal_accepting();
    let mut state = state;
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: previous,
            active: 0,
        })
        .unwrap();
    let (_directory, mut coordinator, _, _, _) =
        from_state((state, previous, previous, build("a"))).await;

    assert_eq!(
        coordinator.current_plan().unwrap(),
        DeployPlan::Retire {
            generation: previous
        }
    );
    assert_eq!(build_attempts(&coordinator, &coordinator.build), 0);
    coordinator
        .record(DeploymentEvent::Retired {
            generation: previous,
        })
        .unwrap();
    assert!(matches!(
        coordinator.plan_for_advance().unwrap(),
        DeployPlan::StageTarget { ref build, .. } if build == &coordinator.build
    ));
}

#[tokio::test]
async fn queued_build_is_idempotent_across_coordinator_crashes() {
    let (_directory, first, previous, target, prior) = fixture(PriorPhase::StageTarget).await;
    let paths = first.paths.clone();
    let persisted_in_progress = first.deployment.clone();
    for _ in 0..2 {
        let coordinator = new_coordinator(paths.clone(), persisted_in_progress.clone()).await;
        assert_eq!(build_attempts(&coordinator, &coordinator.build), 0);
    }

    let mut settled = persisted_in_progress;
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
        DeploymentEvent::TargetAccepting { target },
    ] {
        settled.reconcile(event).unwrap();
    }
    let staged = new_coordinator(paths.clone(), settled).await;
    assert_eq!(build_attempts(&staged, &staged.build), 1);
    assert_eq!(build_attempts(&staged, &prior), 1);
    assert_eq!(staged.active, Some(target));
    assert_ne!(staged.active, Some(previous));

    let recovered = new_coordinator(paths, staged.deployment.clone()).await;
    assert_eq!(build_attempts(&recovered, &recovered.build), 1);
}

#[tokio::test]
async fn current_worker_is_reactivated_while_the_prior_plan_is_recovered() {
    let (_directory, mut coordinator, previous, _target, _prior) =
        fixture(PriorPhase::StageTarget).await;
    let (channel, peer) = channel_pair();
    coordinator.workers.insert(
        previous,
        WorkerSlot {
            registration: 1,
            instance: WorkerInstanceId::new(7).unwrap(),
            ready: true,
            accepting: false,
            draining: false,
            pending_reconciled: true,
            channel,
        },
    );
    let _calls = record_commands(&mut coordinator);

    coordinator.advance().await.unwrap();

    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::Activate { generation }) if generation.raw() == previous.get()
    ));
    coordinator
        .handle_message(
            previous,
            Control::Accepting {
                generation: crate::codex_router::handoff::GenerationId::new(previous.get()),
            },
        )
        .await
        .unwrap();
    assert!(coordinator.accepts_clients());
}

async fn fixture(
    phase: PriorPhase,
) -> (
    tempfile::TempDir,
    Coordinator,
    GenerationId,
    GenerationId,
    BuildId,
) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    let prior_executable = directory.path().join("prior/toks-router");
    std::fs::create_dir_all(prior_executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, b"current-build-b").unwrap();
    std::fs::write(&prior_executable, b"prior-build-a").unwrap();
    let environment = crate::codex_router::systemd::UnitEnvironment::from_values([
        Some("/prior/bin"),
        Some("/prior/home"),
        None,
        None,
    ]);
    let prior = crate::codex_router::systemd::persist_test_launch_contract(
        directory.path(),
        &prior_executable,
        std::path::Path::new("/prior/codex"),
        &environment,
    )
    .unwrap();
    let (mut state, previous) = active_deployment(build("base"));
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(prior.clone()).unwrap() else {
        unreachable!()
    };
    if !matches!(phase, PriorPhase::StageTarget) {
        state
            .reconcile(DeploymentEvent::Prepared { target })
            .unwrap();
    }
    if matches!(
        phase,
        PriorPhase::PreviousPaused | PriorPhase::FailedRollback
    ) {
        state
            .reconcile(DeploymentEvent::PreviousPaused { target })
            .unwrap();
    }
    if matches!(phase, PriorPhase::FailedRollback) {
        state
            .reconcile(DeploymentEvent::Failed {
                generation: target,
                reason: "prior target failed".into(),
            })
            .unwrap();
    }
    let paths = HostPaths {
        executable,
        generations: directory.path().join("generations"),
        control: directory.path().join("control.sock"),
        state: directory.path().join("state.json"),
    };
    let coordinator = new_coordinator(paths, state).await;
    (directory, coordinator, previous, target, prior)
}

async fn from_state(
    state: (DeploymentState, GenerationId, GenerationId, BuildId),
) -> (
    tempfile::TempDir,
    Coordinator,
    GenerationId,
    GenerationId,
    BuildId,
) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"current-build-b").unwrap();
    let paths = HostPaths {
        executable,
        generations: directory.path().join("generations"),
        control: directory.path().join("control.sock"),
        state: directory.path().join("state.json"),
    };
    let coordinator = new_coordinator(paths, state.0).await;
    (directory, coordinator, state.1, state.2, state.3)
}

async fn new_coordinator(paths: HostPaths, state: DeploymentState) -> Coordinator {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    Coordinator::new(listener, paths, state).unwrap()
}

fn settle_prior(coordinator: &mut Coordinator, phase: PriorPhase, target: GenerationId) {
    let events: &[DeploymentEvent] = match phase {
        PriorPhase::StageTarget => &[
            DeploymentEvent::Prepared { target },
            DeploymentEvent::PreviousPaused { target },
            DeploymentEvent::TargetAccepting { target },
        ],
        PriorPhase::Prepared => &[
            DeploymentEvent::PreviousPaused { target },
            DeploymentEvent::TargetAccepting { target },
        ],
        PriorPhase::PreviousPaused => &[DeploymentEvent::TargetAccepting { target }],
        PriorPhase::FailedRollback => &[DeploymentEvent::AdmissionsResumed {
            failed_target: target,
        }],
    };
    for event in events {
        coordinator.deployment.reconcile(event.clone()).unwrap();
    }
}

fn assert_prior_plan(plan: &DeployPlan, phase: PriorPhase, target: GenerationId, prior: &BuildId) {
    let matches = match (phase, plan) {
        (
            PriorPhase::StageTarget,
            DeployPlan::StageTarget {
                target: found,
                build,
            },
        ) => found == &target && build == prior,
        (PriorPhase::Prepared, DeployPlan::PauseAdmissions { target: found, .. })
        | (PriorPhase::PreviousPaused, DeployPlan::StartAccepting { target: found }) => {
            found == &target
        }
        (PriorPhase::FailedRollback, DeployPlan::ResumeAdmissions { failed_target, .. }) => {
            failed_target == &target
        }
        _ => false,
    };
    assert!(matches, "unexpected plan for {phase:?}: {plan:?}");
}

fn terminal_accepting() -> (DeploymentState, GenerationId, GenerationId, BuildId) {
    let (mut state, previous) = active_deployment(build("base"));
    let prior = build("prior-a");
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(prior.clone()).unwrap() else {
        unreachable!()
    };
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
        DeploymentEvent::TargetAccepting { target },
    ] {
        state.reconcile(event).unwrap();
    }
    (state, previous, target, prior)
}

fn failed_before_pause() -> (DeploymentState, GenerationId, GenerationId, BuildId) {
    let (mut state, previous) = active_deployment(build("base"));
    let prior = build("prior-a");
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(prior.clone()).unwrap() else {
        unreachable!()
    };
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Failed {
            generation: target,
            reason: "failed before pause".into(),
        })
        .unwrap();
    (state, previous, target, prior)
}

fn unavailable() -> (DeploymentState, GenerationId, GenerationId, BuildId) {
    let mut state = DeploymentState::default();
    let prior = build("prior-a");
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(prior.clone()).unwrap() else {
        unreachable!()
    };
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Failed {
            generation: target,
            reason: "failed without previous".into(),
        })
        .unwrap();
    (state, target, target, prior)
}

fn active_deployment(build: BuildId) -> (DeploymentState, GenerationId) {
    let mut state = DeploymentState::default();
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(build).unwrap() else {
        unreachable!()
    };
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
        DeploymentEvent::TargetAccepting { target },
    ] {
        state.reconcile(event).unwrap();
    }
    (state, target)
}

fn build(value: &str) -> BuildId {
    BuildId::new(value).unwrap()
}

fn build_attempts(coordinator: &Coordinator, build: &BuildId) -> usize {
    coordinator
        .deployment
        .snapshot()
        .generations
        .iter()
        .filter(|generation| &generation.build == build)
        .count()
}

fn record_commands(coordinator: &mut Coordinator) -> Arc<Mutex<Vec<(&'static str, GenerationId)>>> {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    coordinator.worker_command = Arc::new(move |action, generations| {
        recorded.lock().unwrap().extend(
            generations
                .into_iter()
                .map(|generation| (action, generation)),
        );
        async { Ok(()) }.boxed()
    });
    let inventory = coordinator
        .deployment
        .snapshot()
        .generations
        .into_iter()
        .map(|generation| {
            let liveness = if matches!(
                generation.status,
                crate::codex_router::host::GenerationStatus::Failed
                    | crate::codex_router::host::GenerationStatus::Retired
            ) {
                super::worker_unit::Liveness::Running
            } else {
                super::worker_unit::Liveness::Stopped
            };
            (generation.id, liveness)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    coordinator.worker_inventory = Arc::new(move || {
        let inventory = inventory.clone();
        async move { Ok(inventory) }.boxed()
    });
    calls
}

fn channel_pair() -> (Arc<AsyncChannel>, Arc<AsyncChannel>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pair.sock");
    let listener = HandoffListener::bind(&path).unwrap();
    let peer = HandoffChannel::connect(&path).unwrap();
    let coordinator = listener.accept().unwrap();
    (
        Arc::new(AsyncChannel::new(coordinator).unwrap()),
        Arc::new(AsyncChannel::new(peer).unwrap()),
    )
}
