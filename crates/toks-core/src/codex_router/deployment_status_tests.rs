use tempfile::tempdir;

use super::*;
use crate::accounts::AccountId;
use crate::codex_router::host::{BuildId, DeployPlan, DeploymentEvent, GenerationId};
use crate::rotation::{
    ActiveTask, TaskActivity, ThreadId, ThreadRequestSettings, UnixMillis, WorkerConnectionOwner,
};

#[test]
fn missing_pre_generation_state_stays_quiet() {
    let directory = tempdir().unwrap();
    let status = load_at(
        &directory.path().join("router-host.json"),
        &TaskActivity::default(),
        UnixMillis::new(0),
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

    let mut activity = TaskActivity::default();
    activity
        .replace_worker_at(
            WorkerConnectionOwner::new(old.get(), 101).unwrap(),
            1,
            tasks(&[("oldest", 10), ("newer", 20)]),
            UnixMillis::now(),
        )
        .unwrap();
    activity
        .replace_worker_at(
            WorkerConnectionOwner::new(new.get(), 202).unwrap(),
            1,
            tasks(&[("current", 30)]),
            UnixMillis::now(),
        )
        .unwrap();
    activity
        .reconcile_expected_workers(&std::collections::BTreeMap::from([
            (old.get(), 101),
            (new.get(), 202),
        ]))
        .unwrap();

    let status = project(&state, &activity, UnixMillis::now());
    assert!(status.update_waiting);
    assert_eq!(
        status.generations,
        vec![
            RouterGenerationSummary {
                generation: new.get(),
                build: "new-build".into(),
                role: RouterGenerationRole::Active,
                task_count: Some(1),
                oldest_task_at: Some(UnixMillis::new(30)),
            },
            RouterGenerationSummary {
                generation: old.get(),
                build: "old-build".into(),
                role: RouterGenerationRole::Draining,
                task_count: Some(2),
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

    let status = load_at(&path, &TaskActivity::default(), UnixMillis::new(0)).unwrap();
    assert!(status.update_waiting);
    assert_eq!(status.generations.len(), 1);
    assert_eq!(status.generations[0].generation, pending.get());
    assert_eq!(status.generations[0].role, RouterGenerationRole::Pending);
}

#[test]
fn unavailable_activity_never_falls_back_to_transport_counts() {
    let mut state = DeploymentState::default();
    let active = activate(&mut state, "active");
    let status = project(&state, &TaskActivity::default(), UnixMillis::new(0));

    assert_eq!(status.generations[0].generation, active.get());
    assert_eq!(status.generations[0].task_count, None);
    assert_eq!(status.generations[0].oldest_task_at, None);
}

fn tasks(entries: &[(&str, i64)]) -> std::collections::BTreeMap<ThreadId, ActiveTask> {
    entries
        .iter()
        .map(|(thread, started_at)| {
            (
                ThreadId::new(*thread),
                ActiveTask {
                    account_id: AccountId::new("account"),
                    request_settings: ThreadRequestSettings::default(),
                    started_at: UnixMillis::new(*started_at),
                },
            )
        })
        .collect()
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
