use super::coordinator::Coordinator;
use super::paths::load_state;
use super::test_fixtures::{active_deployment, host_paths};
use crate::codex_router::host::{BuildId, DeployPlan, DeploymentEvent, GenerationStatus};

#[tokio::test]
async fn target_disconnect_after_pause_persists_rollback_before_resuming() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"candidate-build").unwrap();
    let (mut deployment, previous) = active_deployment(BuildId::new("old-build").unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut coordinator = Coordinator::new(
        listener,
        host_paths(directory.path(), executable),
        deployment.clone(),
    )
    .unwrap();
    let target_generation = coordinator
        .deployment
        .snapshot()
        .generations
        .into_iter()
        .find(|generation| generation.status == GenerationStatus::Staged)
        .unwrap();
    let target = target_generation.id;
    let target_build = target_generation.build;
    deployment = coordinator.deployment.clone();
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
    ] {
        deployment.reconcile(event).unwrap();
    }
    coordinator.deployment = deployment;

    coordinator.worker_disconnected(target).unwrap();

    assert!(matches!(
        load_state(&coordinator.paths.state)
            .unwrap()
            .plan_deploy(target_build)
            .unwrap(),
        DeployPlan::ResumeAdmissions {
            previous: found,
            failed_target
        } if found == previous && failed_target == target
    ));
}
