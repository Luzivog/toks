use futures_util::FutureExt;
use std::sync::{Arc, Mutex};

use super::super::test_fixtures::{
    accepting_worker, active_deployment, channel_pair, host_paths, ready_worker,
};
use super::core::Coordinator;
use crate::codex_router::handoff::{Control, WorkerInstanceId};
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId,
};

#[tokio::test]
async fn prepared_cold_start_starts_both_workers_before_pausing_admissions() {
    let (_directory, mut coordinator, previous, target) = fixture(Phase::Prepared).await;
    let calls = record_commands(&mut coordinator);

    coordinator.advance().await.unwrap();

    assert_eq!(
        *calls.lock().unwrap(),
        [("start", previous), ("start", target)]
    );
}

#[tokio::test]
async fn previous_paused_cold_start_restarts_the_target() {
    let (_directory, mut coordinator, previous, target) = fixture(Phase::PreviousPaused).await;
    let calls = record_commands(&mut coordinator);

    coordinator.advance().await.unwrap();

    assert_eq!(
        *calls.lock().unwrap(),
        [("start", previous), ("start", target)]
    );
}

#[tokio::test]
async fn failed_candidate_is_stopped_before_the_previous_worker_restarts() {
    let (_directory, mut coordinator, previous, target) = fixture(Phase::FailedBeforePause).await;
    let calls = record_commands(&mut coordinator);

    coordinator.advance().await.unwrap();

    assert_eq!(
        *calls.lock().unwrap(),
        [("stop", target), ("start", previous)]
    );
}

#[tokio::test]
async fn reinstall_retry_intent_survives_rollback_until_a_fresh_generation_is_staged() {
    let (_directory, coordinator, _previous, failed) = fixture(Phase::FailedAfterPause).await;
    let paths = coordinator.paths.clone();
    let build = coordinator.build.clone();
    let intent = crate::codex_router::host::request_retry(&paths.state, &build).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let during_rollback =
        Coordinator::new(listener, paths.clone(), coordinator.deployment).unwrap();
    assert!(during_rollback.consumed_retry_intent.is_none());
    assert_eq!(
        crate::codex_router::host::load_retry_intent(&paths.state).unwrap(),
        Some(intent)
    );

    let mut retried = during_rollback;
    retried
        .deployment
        .reconcile(DeploymentEvent::AdmissionsResumed {
            failed_target: failed,
        })
        .unwrap();
    assert!(retried.consume_retry_intent().unwrap());
    assert_eq!(
        crate::codex_router::host::load_retry_intent(&paths.state).unwrap(),
        None
    );
    assert!(retried
        .deployment
        .snapshot()
        .generations
        .iter()
        .any(|item| {
            item.build == build
                && item.id != failed
                && item.status == crate::codex_router::host::GenerationStatus::Staged
        }));
}

#[tokio::test]
async fn build_a_crash_cannot_consume_build_b_retry_intent() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    let paths = host_paths(directory.path(), executable.clone());

    std::fs::write(&executable, b"build-b").unwrap();
    let build_b = paths.build_id().unwrap();
    let mut persisted = DeploymentState::default();
    let DeployPlan::StageTarget {
        target: failed_b, ..
    } = persisted.plan_deploy(build_b.clone()).unwrap()
    else {
        panic!("expected initial B deployment")
    };
    persisted
        .reconcile(DeploymentEvent::Failed {
            generation: failed_b,
            reason: "failed B".into(),
        })
        .unwrap();
    let intent_b = crate::codex_router::host::request_retry(&paths.state, &build_b).unwrap();

    std::fs::write(&executable, b"build-a").unwrap();
    let listener_a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let coordinator_a = Coordinator::new(listener_a, paths.clone(), persisted.clone()).unwrap();
    assert_ne!(coordinator_a.build, build_b);
    assert!(coordinator_a.consumed_retry_intent.is_none());
    assert_eq!(
        crate::codex_router::host::load_retry_intent(&paths.state).unwrap(),
        Some(intent_b)
    );
    drop(coordinator_a); // Crash before A's in-memory deployment plan is persisted.

    std::fs::write(&executable, b"build-b").unwrap();
    let listener_b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let coordinator_b = Coordinator::new(listener_b, paths, persisted).unwrap();
    assert!(coordinator_b.consumed_retry_intent.is_some());
    assert!(coordinator_b
        .deployment
        .snapshot()
        .generations
        .iter()
        .any(|generation| {
            generation.build == build_b
                && generation.id != failed_b
                && generation.status == crate::codex_router::host::GenerationStatus::Staged
        }));
}

#[tokio::test]
async fn target_accepting_timeout_persists_rollback_and_stops_the_candidate() {
    use super::wait::WaitKey;
    use std::time::Duration;

    let (_directory, mut coordinator, previous, target) = fixture(Phase::PreviousPaused).await;
    let calls = record_commands(&mut coordinator);
    coordinator.deployment_wait.arm(
        WaitKey::TargetAccepting(target),
        tokio::time::Instant::now() - Duration::from_secs(9),
    );

    coordinator.expire_waits().await.unwrap();

    assert!(matches!(
        coordinator
            .deployment
            .plan_deploy(coordinator.build.clone())
            .unwrap(),
        DeployPlan::ResumeAdmissions {
            previous: found,
            failed_target
        } if found == previous && failed_target == target
    ));
    assert_eq!(*calls.lock().unwrap(), [("stop", target)]);
}

#[tokio::test]
async fn rollback_accepting_timeout_retries_without_stopping_the_previous_worker() {
    use super::wait::WaitKey;
    use std::time::Duration;

    let (_directory, mut coordinator, previous, _target) = fixture(Phase::FailedAfterPause).await;
    let calls = record_commands(&mut coordinator);
    coordinator.deployment_wait.arm(
        WaitKey::AdmissionsResumed(previous),
        tokio::time::Instant::now() - Duration::from_secs(9),
    );

    coordinator.expire_waits().await.unwrap();

    assert!(matches!(
        coordinator
            .deployment
            .plan_deploy(coordinator.build.clone())
            .unwrap(),
        DeployPlan::ResumeAdmissions {
            previous: found,
            ..
        } if found == previous
    ));
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unrelated_worker_ready_timeout_does_not_fail_the_candidate() {
    use super::wait::WaitKey;
    use std::time::Duration;

    let (_directory, mut coordinator, previous, target) = fixture(Phase::Prepared).await;
    coordinator.deployment_wait.arm(
        WaitKey::WorkerReady(previous),
        tokio::time::Instant::now() - Duration::from_secs(9),
    );

    coordinator.expire_waits().await.unwrap();

    assert!(matches!(
        coordinator
            .deployment
            .plan_deploy(coordinator.build.clone())
            .unwrap(),
        DeployPlan::PauseAdmissions {
            previous: Some(found_previous),
            target: found_target,
        } if found_previous == previous && found_target == target
    ));
}

#[tokio::test]
async fn paused_draining_worker_is_not_commanded_to_drain_again() {
    let (_directory, mut coordinator, previous, _target) = fixture(Phase::Prepared).await;
    let (channel, peer) = channel_pair();
    coordinator.workers.insert(
        previous,
        accepting_worker(channel, 1, WorkerInstanceId::new(1).unwrap()),
    );
    coordinator
        .handle_message(
            previous,
            Control::AdmissionsPaused {
                generation: crate::codex_router::handoff::GenerationId::new(previous.get()),
            },
        )
        .await
        .unwrap();

    coordinator.reconcile_workers().await.unwrap();

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(75), peer.receive())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn owner_reconciliation_waits_for_every_live_generation() {
    let (_directory, mut coordinator, previous, target) = fixture(Phase::Prepared).await;
    let (previous_channel, _previous_peer) = channel_pair();
    coordinator.workers.insert(
        previous,
        ready_worker(previous_channel, 1, WorkerInstanceId::new(101).unwrap()),
    );
    assert!(coordinator.reconcilable_worker_instances().is_none());

    let (target_channel, _target_peer) = channel_pair();
    coordinator.workers.insert(
        target,
        ready_worker(target_channel, 1, WorkerInstanceId::new(202).unwrap()),
    );
    assert_eq!(
        coordinator.reconcilable_worker_instances().unwrap(),
        std::collections::BTreeMap::from([(previous.get(), 101), (target.get(), 202)])
    );
}

#[test]
fn every_activation_acknowledgement_has_a_bounded_wait() {
    use super::wait::{DeploymentWait, WaitKey};
    use std::time::Duration;

    let now = tokio::time::Instant::now();
    let generation = GenerationId::from_raw(3);
    for key in [
        WaitKey::WorkerReady(generation),
        WaitKey::AdmissionsPaused(generation),
        WaitKey::TargetAccepting(generation),
        WaitKey::AdmissionsResumed(generation),
    ] {
        let mut wait = DeploymentWait::default();
        wait.arm(key, now);
        assert!(wait.take_expired(now + Duration::from_secs(7)).is_empty());
        assert_eq!(wait.take_expired(now + Duration::from_secs(8)), [key]);
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Prepared,
    PreviousPaused,
    FailedBeforePause,
    FailedAfterPause,
}

async fn fixture(phase: Phase) -> (tempfile::TempDir, Coordinator, GenerationId, GenerationId) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"candidate-build").unwrap();
    let paths = host_paths(directory.path(), executable);
    let candidate = paths.build_id().unwrap();
    let (mut deployment, previous) = active_deployment(BuildId::new("old-build").unwrap());
    let DeployPlan::StageTarget { target, .. } = deployment.plan_deploy(candidate).unwrap() else {
        panic!("expected candidate target")
    };
    deployment
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    match phase {
        Phase::Prepared => {}
        Phase::PreviousPaused => {
            deployment
                .reconcile(DeploymentEvent::PreviousPaused { target })
                .unwrap();
        }
        Phase::FailedBeforePause => {
            deployment
                .reconcile(DeploymentEvent::Failed {
                    generation: target,
                    reason: "candidate failed".into(),
                })
                .unwrap();
        }
        Phase::FailedAfterPause => {
            deployment
                .reconcile(DeploymentEvent::PreviousPaused { target })
                .unwrap();
            deployment
                .reconcile(DeploymentEvent::Failed {
                    generation: target,
                    reason: "candidate failed".into(),
                })
                .unwrap();
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let coordinator = Coordinator::new(listener, paths, deployment).unwrap();
    (directory, coordinator, previous, target)
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
