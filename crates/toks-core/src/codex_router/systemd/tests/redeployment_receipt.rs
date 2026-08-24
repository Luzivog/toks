use std::fs;

use tempfile::tempdir;

use super::super::receipt::failed_candidate;
use super::activate;
use crate::codex_router::host::{BuildId, DeployPlan, DeploymentEvent, DeploymentState, RetryId};

#[test]
fn retired_retry_shadows_older_failure_across_install_and_host_planning() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("state.json");
    let build_a = BuildId::new("build-a").unwrap();
    let mut state = DeploymentState::default();
    let DeployPlan::StageTarget { target: failed, .. } =
        state.plan_deploy(build_a.clone()).unwrap()
    else {
        unreachable!()
    };
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "first attempt failed".into(),
        })
        .unwrap();
    state
        .consume_retry(build_a.clone(), RetryId::for_test(4))
        .unwrap();
    let retry = activate(&mut state, build_a.clone());
    activate(&mut state, BuildId::new("build-b").unwrap());
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: retry,
            active: 0,
        })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Retired { generation: retry })
        .unwrap();
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();

    assert!(!failed_candidate(&path, &build_a).unwrap());
    assert!(matches!(
        state.plan_deploy(build_a),
        Ok(DeployPlan::StageTarget { target, .. }) if target != failed && target != retry
    ));
}
