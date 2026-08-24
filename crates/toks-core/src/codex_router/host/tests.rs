use super::model::{ActivationPhase, DeployError};
use super::*;

mod redeployment;
mod retry_identity;

fn build(value: &str) -> BuildId {
    BuildId::new(value).unwrap()
}

fn stage(state: &mut DeploymentState, name: &str) -> GenerationId {
    let plan = state.plan_deploy(build(name)).unwrap();
    let DeployPlan::StageTarget { target, .. } = plan else {
        panic!("expected staging plan, got {plan:?}");
    };
    target
}

fn activate_first(state: &mut DeploymentState, name: &str) -> GenerationId {
    let target = stage(state, name);
    assert_eq!(
        state
            .reconcile(DeploymentEvent::Prepared { target })
            .unwrap(),
        DeployPlan::PauseAdmissions {
            previous: None,
            target,
        }
    );
    assert_eq!(
        state
            .reconcile(DeploymentEvent::PreviousPaused { target })
            .unwrap(),
        DeployPlan::StartAccepting { target }
    );
    assert_eq!(
        state
            .reconcile(DeploymentEvent::TargetAccepting { target })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(target),
        }
    );
    target
}

#[test]
fn first_deployment_replays_every_crash_phase_idempotently() {
    let mut state = DeploymentState::default();
    let target = stage(&mut state, "build-a");
    assert_eq!(
        state.plan_deploy(build("build-a")).unwrap(),
        DeployPlan::StageTarget {
            target,
            build: build("build-a"),
        }
    );

    let paused = state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    assert_eq!(state.plan_deploy(build("build-a")).unwrap(), paused);
    assert_eq!(
        serde_json::from_str::<DeploymentState>(&serde_json::to_string(&state).unwrap())
            .unwrap()
            .reconcile(DeploymentEvent::Prepared { target })
            .unwrap(),
        paused
    );

    let accepting = state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    assert_eq!(state.plan_deploy(build("build-a")).unwrap(), accepting);
    assert_eq!(
        state
            .reconcile(DeploymentEvent::PreviousPaused { target })
            .unwrap(),
        accepting
    );
    let settled = state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
    assert_eq!(state.plan_deploy(build("build-a")).unwrap(), settled);
    assert_eq!(
        state
            .reconcile(DeploymentEvent::TargetAccepting { target })
            .unwrap(),
        settled
    );
    assert_eq!(
        state.snapshot().activation.unwrap().phase,
        ActivationPhase::TargetAccepting
    );
}

#[test]
fn draining_requires_a_fresh_zero_after_admissions_pause() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: previous,
            active: 0,
        })
        .unwrap();

    let target = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    let previous_snapshot = state
        .snapshot()
        .generations
        .into_iter()
        .find(|generation| generation.id == previous)
        .unwrap();
    assert_eq!(previous_snapshot.status, GenerationStatus::Draining);
    assert_eq!(previous_snapshot.active_connections, None);
    assert_eq!(
        state
            .reconcile(DeploymentEvent::TargetAccepting { target })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(target),
        }
    );

    assert_eq!(
        state
            .reconcile(DeploymentEvent::ConnectionsObserved {
                generation: previous,
                active: 0,
            })
            .unwrap(),
        DeployPlan::Retire {
            generation: previous,
        }
    );
    assert_eq!(
        state
            .reconcile(DeploymentEvent::Retired {
                generation: previous,
            })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(target),
        }
    );
}

#[test]
fn nonzero_draining_generation_is_never_retired() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let target = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
    assert_eq!(
        state
            .reconcile(DeploymentEvent::ConnectionsObserved {
                generation: previous,
                active: 9,
            })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(target),
        }
    );
    let before = state.snapshot();
    assert!(matches!(
        state.reconcile(DeploymentEvent::Retired {
            generation: previous,
        }),
        Err(DeployError::InvalidTransition(_))
    ));
    assert_eq!(state.snapshot(), before);
}

#[test]
fn failure_after_pause_replays_rollback_until_previous_accepts() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let target = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    let rollback = state
        .reconcile(DeploymentEvent::Failed {
            generation: target,
            reason: "readiness failed".into(),
        })
        .unwrap();
    assert_eq!(
        rollback,
        DeployPlan::ResumeAdmissions {
            previous,
            failed_target: target,
        }
    );
    let mut restored: DeploymentState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(
        restored
            .reconcile(DeploymentEvent::Failed {
                generation: target,
                reason: "duplicate report".into(),
            })
            .unwrap(),
        rollback
    );
    assert_eq!(
        restored
            .reconcile(DeploymentEvent::AdmissionsResumed {
                failed_target: target,
            })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(previous),
        }
    );
    assert!(restored
        .reconcile(DeploymentEvent::AdmissionsResumed {
            failed_target: target,
        })
        .is_ok());
    assert_eq!(restored.snapshot().last_rollback, Some(target));
}

#[test]
fn failure_before_pause_leaves_previous_active() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let target = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    assert_eq!(
        state
            .reconcile(DeploymentEvent::Failed {
                generation: target,
                reason: "bad artifact".into(),
            })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(previous),
        }
    );
    assert_eq!(
        state.plan_deploy(build("build-b")).unwrap(),
        DeployPlan::Unavailable {
            failed_target: target,
            active: Some(previous),
        }
    );
}

#[test]
fn different_build_is_rejected_during_incomplete_handoff() {
    let mut state = DeploymentState::default();
    stage(&mut state, "build-a");
    assert_eq!(
        state.plan_deploy(build("build-b")),
        Err(DeployError::DeploymentBusy)
    );
}

#[test]
fn a_new_deployment_does_not_confuse_an_older_draining_generation() {
    let mut state = DeploymentState::default();
    let oldest = activate_first(&mut state, "build-a");
    let middle = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target: middle })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target: middle })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target: middle })
        .unwrap();
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: oldest,
            active: 3,
        })
        .unwrap();

    let newest = stage(&mut state, "build-c");
    state
        .reconcile(DeploymentEvent::Prepared { target: newest })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target: newest })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target: newest })
        .unwrap();

    let snapshot = state.snapshot();
    let status = |id| {
        snapshot
            .generations
            .iter()
            .find(|generation| generation.id == id)
            .unwrap()
            .status
    };
    assert_eq!(status(oldest), GenerationStatus::Draining);
    assert_eq!(status(middle), GenerationStatus::Draining);
    assert_eq!(status(newest), GenerationStatus::Active);
    assert_eq!(
        state
            .reconcile(DeploymentEvent::ConnectionsObserved {
                generation: oldest,
                active: 0,
            })
            .unwrap(),
        DeployPlan::Retire { generation: oldest }
    );
}

#[test]
fn target_can_take_over_if_previous_fails_during_handoff() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let target = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    assert_eq!(
        state
            .reconcile(DeploymentEvent::Failed {
                generation: previous,
                reason: "worker exited".into(),
            })
            .unwrap(),
        DeployPlan::PauseAdmissions {
            previous: Some(previous),
            target,
        }
    );
    assert_eq!(
        state
            .reconcile(DeploymentEvent::PreviousPaused { target })
            .unwrap(),
        DeployPlan::StartAccepting { target }
    );
    assert_eq!(
        state
            .reconcile(DeploymentEvent::TargetAccepting { target })
            .unwrap(),
        DeployPlan::Settled {
            active: Some(target),
        }
    );
}

#[test]
fn persisted_phase_names_and_invalid_state_are_detectable() {
    let mut state = DeploymentState::default();
    let target = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    let mut json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["activation"]["phase"], "previousPaused");
    json["nextGeneration"] = 0.into();
    let invalid: DeploymentState = serde_json::from_value(json).unwrap();
    assert_eq!(
        invalid.validate(),
        Err(DeployError::InvalidPersistedState("invalid generation id"))
    );
}

#[test]
fn blank_build_ids_are_rejected() {
    assert_eq!(BuildId::new("  "), Err(DeployError::InvalidBuildId));
}

#[test]
fn recovered_state_allocates_after_existing_generation_directories() {
    let mut state = DeploymentState::default();
    state.reserve_generation_ids_after(41).unwrap();
    assert!(matches!(
        state.plan_deploy(build("recovered-build")).unwrap(),
        DeployPlan::StageTarget { target, .. } if target.get() == 42
    ));
}

#[test]
fn stale_rollback_and_retired_connection_reports_are_rejected() {
    let mut state = DeploymentState::default();
    let active = activate_first(&mut state, "build-a");
    assert!(matches!(
        state.reconcile(DeploymentEvent::AdmissionsResumed {
            failed_target: active,
        }),
        Err(DeployError::InvalidTransition(_))
    ));

    let target = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: active,
            active: 0,
        })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Retired { generation: active })
        .unwrap();
    assert!(matches!(
        state.reconcile(DeploymentEvent::ConnectionsObserved {
            generation: active,
            active: 1,
        }),
        Err(DeployError::InvalidTransition(_))
    ));
}
