use tempfile::tempdir;

use super::*;
use crate::accounts::AccountId;
use crate::codex_router::host::{BuildId, DeployPlan, DeploymentEvent, GenerationId};
use crate::rotation::{ThreadId, UnixMillis, WorkerConnectionOwner};

#[test]
fn missing_pre_generation_state_stays_quiet() {
    let directory = tempdir().unwrap();
    let status = load_at(
        &directory.path().join("router-host.json"),
        &RotationRuntime::default(),
    )
    .unwrap();

    assert_eq!(status, RouterDeploymentStatus::default());
}

#[test]
fn projection_reports_builds_and_unique_live_tasks_per_generation() {
    let mut state = DeploymentState::default();
    let old = activate(&mut state, "old-build");
    let new = stage(&mut state, "new-build");
    state
        .reconcile(DeploymentEvent::Prepared { target: new })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target: new })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target: new })
        .unwrap();

    let account = AccountId::new("account");
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened_by(
            WorkerConnectionOwner::new(old.get(), 101).unwrap(),
            &account,
            &ThreadId::new("oldest"),
            UnixMillis::new(10),
        )
        .unwrap();
    runtime
        .connection_opened_by(
            WorkerConnectionOwner::new(old.get(), 101).unwrap(),
            &account,
            &ThreadId::new("oldest"),
            UnixMillis::new(11),
        )
        .unwrap();
    runtime
        .connection_opened_by(
            WorkerConnectionOwner::new(old.get(), 101).unwrap(),
            &account,
            &ThreadId::new("newer"),
            UnixMillis::new(20),
        )
        .unwrap();
    runtime
        .connection_opened_by(
            WorkerConnectionOwner::new(new.get(), 202).unwrap(),
            &account,
            &ThreadId::new("current"),
            UnixMillis::new(30),
        )
        .unwrap();

    let status = project(&state, &runtime);
    assert!(status.update_waiting);
    assert_eq!(
        status.generations,
        vec![
            RouterGenerationSummary {
                generation: new.get(),
                build: "new-build".into(),
                role: RouterGenerationRole::Active,
                task_count: 1,
                oldest_task_at: Some(UnixMillis::new(30)),
            },
            RouterGenerationSummary {
                generation: old.get(),
                build: "old-build".into(),
                role: RouterGenerationRole::Draining,
                task_count: 2,
                oldest_task_at: Some(UnixMillis::new(10)),
            },
        ]
    );
}

#[test]
fn persisted_state_is_the_read_model_authority() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("router-host.json");
    let mut state = DeploymentState::default();
    let pending = stage(&mut state, "candidate");
    std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    let status = load_at(&path, &RotationRuntime::default()).unwrap();
    assert!(status.update_waiting);
    assert_eq!(status.generations.len(), 1);
    assert_eq!(status.generations[0].generation, pending.get());
    assert_eq!(status.generations[0].role, RouterGenerationRole::Pending);
}

fn stage(state: &mut DeploymentState, build: &str) -> GenerationId {
    let DeployPlan::StageTarget { target, .. } =
        state.plan_deploy(BuildId::new(build).unwrap()).unwrap()
    else {
        panic!("expected staged generation");
    };
    target
}

fn activate(state: &mut DeploymentState, build: &str) -> GenerationId {
    let target = stage(state, build);
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
    target
}
