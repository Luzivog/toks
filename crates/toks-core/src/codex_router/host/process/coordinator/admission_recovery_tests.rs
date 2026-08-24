use futures_util::FutureExt;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::core::Coordinator;
use super::wait::WaitKey;
use super::worker_unit::Liveness;
use crate::codex_router::handoff::{Control, Received, WorkerInstanceId};
use crate::codex_router::host::process::channel::AsyncChannel;
use crate::codex_router::host::process::test_fixtures::{
    accepting_worker, active_deployment, channel_pair, host_paths, ready_worker,
};
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId, GenerationStatus,
};

#[tokio::test]
async fn delayed_pause_ack_never_reopens_the_previous_worker() {
    let (_directory, mut coordinator, previous, target) = prepared_fixture().await;
    let (previous_channel, previous_peer) = channel_pair();
    let (target_channel, _target_peer) = channel_pair();
    coordinator.workers.replace(
        previous,
        accepting_worker(previous_channel, 1, WorkerInstanceId::new(1).unwrap()),
    );
    coordinator.workers.replace(
        target,
        ready_worker(target_channel, 2, WorkerInstanceId::new(2).unwrap()),
    );

    coordinator.advance().await.unwrap();
    assert!(matches!(
        previous_peer.receive().await.unwrap(),
        Received::Control(Control::Drain { generation }) if generation.raw() == previous.get()
    ));
    assert!(coordinator
        .deployment_wait
        .is_armed(WaitKey::AdmissionsPaused(previous)));

    coordinator.advance().await.unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(75), previous_peer.receive())
            .await
            .is_err()
    );
    assert!(!coordinator.accepts_clients());

    coordinator
        .deployment_wait
        .acknowledge(WaitKey::AdmissionsPaused(previous));
    coordinator.deployment_wait.arm(
        WaitKey::AdmissionsPaused(previous),
        tokio::time::Instant::now() - Duration::from_secs(9),
    );
    coordinator.expire_waits().await.unwrap();
    coordinator.advance().await.unwrap();
    assert!(matches!(
        previous_peer.receive().await.unwrap(),
        Received::Control(Control::Drain { generation }) if generation.raw() == previous.get()
    ));
}

#[tokio::test]
async fn confirmed_previous_loss_advances_the_ready_target_without_stopping_sockets() {
    let (_directory, mut coordinator, previous, target) = prepared_fixture().await;
    let calls = record_commands(&mut coordinator);
    let target_peer = disconnect_previous_while_pausing(&mut coordinator, previous, target).await;
    set_inventory(
        &mut coordinator,
        BTreeMap::from([(previous, Liveness::Stopped)]),
    );
    expire_previous_ready(&mut coordinator, previous).await;

    let snapshot = coordinator.deployment.snapshot();
    assert!(snapshot.generations.iter().any(|generation| {
        generation.id == previous && generation.status == GenerationStatus::Failed
    }));
    assert!(matches!(
        coordinator.current_plan().unwrap(),
        DeployPlan::StartAccepting { target: found } if found == target
    ));
    assert!(!calls
        .lock()
        .unwrap()
        .iter()
        .any(|(action, generation)| *action == "stop" && *generation == previous));

    coordinator.advance().await.unwrap();
    assert!(matches!(
        target_peer.receive().await.unwrap(),
        Received::Control(Control::Activate { generation }) if generation.raw() == target.get()
    ));
}

#[tokio::test]
async fn unknown_liveness_evidence_keeps_the_target_closed() {
    let (_directory, mut coordinator, previous, target) = prepared_fixture().await;
    let target_peer = disconnect_previous_while_pausing(&mut coordinator, previous, target).await;
    set_inventory(
        &mut coordinator,
        BTreeMap::from([(previous, Liveness::Unknown)]),
    );
    expire_previous_ready(&mut coordinator, previous).await;

    assert!(matches!(
        coordinator.current_plan().unwrap(),
        DeployPlan::PauseAdmissions {
            previous: Some(found_previous),
            target: found_target,
        } if found_previous == previous && found_target == target
    ));
    assert!(coordinator
        .deployment
        .snapshot()
        .generations
        .iter()
        .any(|generation| {
            generation.id == previous && generation.status == GenerationStatus::Active
        }));
    coordinator.advance().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(75), target_peer.receive())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn crash_after_recording_previous_failure_resumes_from_durable_state() {
    let (_directory, coordinator, previous, target) = prepared_fixture().await;
    let paths = coordinator.paths.clone();
    let mut deployment = coordinator.deployment;
    deployment
        .reconcile(DeploymentEvent::Failed {
            generation: previous,
            reason: "confirmed dead before coordinator crash".into(),
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut recovered = Coordinator::new(listener, paths, deployment).unwrap();
    let (target_channel, target_peer) = channel_pair();
    recovered.workers.replace(
        target,
        ready_worker(target_channel, 2, WorkerInstanceId::new(2).unwrap()),
    );

    recovered.advance().await.unwrap();

    assert!(matches!(
        target_peer.receive().await.unwrap(),
        Received::Control(Control::Activate { generation }) if generation.raw() == target.get()
    ));
}

#[tokio::test]
async fn cold_start_reestablishes_bounded_control_absence_before_takeover() {
    let (_directory, mut coordinator, previous, target) = prepared_fixture().await;
    assert!(coordinator.workers.is_disconnected(previous));
    let _calls = record_commands(&mut coordinator);
    set_inventory(
        &mut coordinator,
        BTreeMap::from([(previous, Liveness::Stopped)]),
    );
    let (target_channel, target_peer) = channel_pair();
    coordinator.workers.replace(
        target,
        ready_worker(target_channel, 2, WorkerInstanceId::new(2).unwrap()),
    );

    coordinator.advance().await.unwrap();
    expire_previous_ready(&mut coordinator, previous).await;
    coordinator.advance().await.unwrap();

    assert!(matches!(
        target_peer.receive().await.unwrap(),
        Received::Control(Control::Activate { generation }) if generation.raw() == target.get()
    ));
}

#[tokio::test]
async fn huge_terminal_history_and_a_stalled_stop_obey_one_aggregate_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"current-build").unwrap();
    let paths = host_paths(directory.path(), executable);
    let mut state = DeploymentState::default();
    for index in 0..1_000 {
        let DeployPlan::StageTarget { target, .. } = state
            .plan_deploy(BuildId::new(format!("failed-{index}")).unwrap())
            .unwrap()
        else {
            unreachable!()
        };
        state
            .reconcile(DeploymentEvent::Failed {
                generation: target,
                reason: "synthetic failure".into(),
            })
            .unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut coordinator = Coordinator::new(listener, paths, state).unwrap();
    let loaded = coordinator
        .deployment
        .snapshot()
        .generations
        .iter()
        .take(2)
        .map(|generation| (generation.id, Liveness::Stopped))
        .collect::<BTreeMap<_, _>>();
    set_inventory(&mut coordinator, loaded);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    coordinator.worker_command = Arc::new(move |action, generations| {
        recorded.lock().unwrap().push((action, generations));
        std::future::pending::<anyhow::Result<()>>().boxed()
    });

    let error = coordinator
        .reconcile_worker_units_with_timeout(Duration::from_millis(75))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("timed out"));
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "stop");
    assert_eq!(calls[0].1.len(), 2);
}

async fn prepared_fixture() -> (tempfile::TempDir, Coordinator, GenerationId, GenerationId) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"candidate-build").unwrap();
    let paths = host_paths(directory.path(), executable);
    let candidate = paths.build_id().unwrap();
    let (mut deployment, previous) = active_deployment(BuildId::new("old-build").unwrap());
    let DeployPlan::StageTarget { target, .. } = deployment.plan_deploy(candidate).unwrap() else {
        unreachable!()
    };
    deployment
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let coordinator = Coordinator::new(listener, paths, deployment).unwrap();
    (directory, coordinator, previous, target)
}

async fn disconnect_previous_while_pausing(
    coordinator: &mut Coordinator,
    previous: GenerationId,
    target: GenerationId,
) -> Arc<AsyncChannel> {
    let (previous_channel, previous_peer) = channel_pair();
    let (target_channel, target_peer) = channel_pair();
    coordinator.workers.replace(
        previous,
        accepting_worker(previous_channel, 1, WorkerInstanceId::new(1).unwrap()),
    );
    coordinator.workers.replace(
        target,
        ready_worker(target_channel, 2, WorkerInstanceId::new(2).unwrap()),
    );
    coordinator.advance().await.unwrap();
    assert!(matches!(
        previous_peer.receive().await.unwrap(),
        Received::Control(Control::Drain { .. })
    ));
    coordinator.workers.remove_registered(previous);
    coordinator.deployment_wait.clear_generation(previous);
    coordinator.worker_disconnected(previous).unwrap();
    coordinator.advance().await.unwrap();
    target_peer
}

async fn expire_previous_ready(coordinator: &mut Coordinator, previous: GenerationId) {
    coordinator
        .deployment_wait
        .acknowledge(WaitKey::WorkerReady(previous));
    coordinator.deployment_wait.arm(
        WaitKey::WorkerReady(previous),
        tokio::time::Instant::now() - Duration::from_secs(9),
    );
    coordinator.expire_waits().await.unwrap();
}

fn set_inventory(coordinator: &mut Coordinator, inventory: BTreeMap<GenerationId, Liveness>) {
    coordinator.worker_inventory = Arc::new(move || {
        let inventory = inventory.clone();
        async move { Ok(inventory) }.boxed()
    });
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
    calls
}
