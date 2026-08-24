use super::*;

fn complete_activation(state: &mut DeploymentState, target: GenerationId) {
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
}

fn replayed_plan(state: &DeploymentState, build_id: &str) -> DeployPlan {
    let mut recovered: DeploymentState =
        serde_json::from_str(&serde_json::to_string(state).unwrap()).unwrap();
    recovered.plan_deploy(build(build_id)).unwrap()
}

#[test]
fn persisted_handoff_phases_replay_the_same_next_action() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let target = stage(&mut state, "build-b");
    assert!(matches!(
        replayed_plan(&state, "build-b"),
        DeployPlan::StageTarget { target: found, .. } if found == target
    ));

    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    assert_eq!(
        replayed_plan(&state, "build-b"),
        DeployPlan::PauseAdmissions {
            previous: Some(previous),
            target,
        }
    );

    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    assert_eq!(
        replayed_plan(&state, "build-b"),
        DeployPlan::StartAccepting { target }
    );

    state
        .reconcile(DeploymentEvent::TargetAccepting { target })
        .unwrap();
    assert_eq!(
        replayed_plan(&state, "build-b"),
        DeployPlan::Settled {
            active: Some(target),
        }
    );

    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: previous,
            active: 0,
        })
        .unwrap();
    assert_eq!(
        replayed_plan(&state, "build-b"),
        DeployPlan::Retire {
            generation: previous,
        }
    );
}

#[test]
fn redeploying_a_retired_build_creates_a_fresh_generation() {
    let mut state = DeploymentState::default();
    let first_a = activate_first(&mut state, "build-a");
    let b = stage(&mut state, "build-b");
    complete_activation(&mut state, b);
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: first_a,
            active: 0,
        })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Retired {
            generation: first_a,
        })
        .unwrap();
    let mut state: DeploymentState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();

    let DeployPlan::StageTarget {
        target: second_a,
        build: target_build,
    } = state.plan_deploy(build("build-a")).unwrap()
    else {
        panic!("retired build must be staged as a new generation");
    };
    assert_ne!(second_a, first_a);
    assert_eq!(target_build, build("build-a"));

    complete_activation(&mut state, second_a);
    assert_eq!(
        state.plan_deploy(build("build-a")).unwrap(),
        DeployPlan::Settled {
            active: Some(second_a),
        }
    );
}

#[test]
fn newest_retired_attempt_shadows_older_failure_when_redeploying_same_build() {
    let mut state = DeploymentState::default();
    let first_a = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: first_a,
            reason: "first attempt failed".into(),
        })
        .unwrap();
    assert!(state
        .consume_retry(build("build-a"), RetryId::for_test(1))
        .unwrap());
    let second_a = state
        .snapshot()
        .generations
        .iter()
        .find(|generation| {
            generation.build == build("build-a") && generation.status == GenerationStatus::Staged
        })
        .unwrap()
        .id;
    complete_activation(&mut state, second_a);

    let b = stage(&mut state, "build-b");
    complete_activation(&mut state, b);
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: second_a,
            active: 0,
        })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Retired {
            generation: second_a,
        })
        .unwrap();
    let mut recovered: DeploymentState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();

    let DeployPlan::StageTarget {
        target: third_a,
        build: target_build,
    } = recovered.plan_deploy(build("build-a")).unwrap()
    else {
        panic!("the newest retired attempt must shadow older failed history");
    };
    assert_ne!(third_a, first_a);
    assert_ne!(third_a, second_a);
    assert_eq!(target_build, build("build-a"));
}

#[test]
fn active_retry_remains_current_despite_older_failure() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "first attempt failed".into(),
        })
        .unwrap();
    let Some(DeployPlan::StageTarget { target: retry, .. }) =
        state.retry_deploy(build("build-a")).unwrap()
    else {
        panic!("retry must stage a fresh generation");
    };
    complete_activation(&mut state, retry);

    assert_eq!(
        state.plan_deploy(build("build-a")).unwrap(),
        DeployPlan::Settled {
            active: Some(retry)
        }
    );
}

#[test]
fn newest_failed_attempt_remains_authoritative_over_older_retired_attempt() {
    let mut state = DeploymentState::default();
    let first_a = activate_first(&mut state, "build-a");
    let b = stage(&mut state, "build-b");
    complete_activation(&mut state, b);
    state
        .reconcile(DeploymentEvent::ConnectionsObserved {
            generation: first_a,
            active: 0,
        })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Retired {
            generation: first_a,
        })
        .unwrap();
    let failed_a = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed_a,
            reason: "new deployment failed".into(),
        })
        .unwrap();

    assert_eq!(
        state.plan_deploy(build("build-a")).unwrap(),
        DeployPlan::Unavailable {
            failed_target: failed_a,
            active: Some(b),
        }
    );
}

#[test]
fn newest_draining_attempt_shadows_older_failure_when_redeploying_same_build() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "first attempt failed".into(),
        })
        .unwrap();
    let Some(DeployPlan::StageTarget { target: retry, .. }) =
        state.retry_deploy(build("build-a")).unwrap()
    else {
        panic!("retry must stage a fresh generation");
    };
    complete_activation(&mut state, retry);
    let b = stage(&mut state, "build-b");
    complete_activation(&mut state, b);

    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(build("build-a")).unwrap()
    else {
        panic!("draining attempt must shadow the obsolete failure");
    };
    assert_ne!(target, failed);
    assert_ne!(target, retry);
}

#[test]
fn terminal_failed_activation_does_not_block_a_later_deployment() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Prepared { target: failed })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target: failed })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "worker exited".into(),
        })
        .unwrap();

    let recovered: DeploymentState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert!(matches!(
        recovered.clone().plan_deploy(build("build-a")),
        Ok(DeployPlan::Unavailable {
            failed_target,
            active: None,
        }) if failed_target == failed
    ));
    assert!(matches!(
        recovered.clone().plan_deploy(build("build-b")),
        Ok(DeployPlan::StageTarget { build: target_build, .. })
            if target_build == build("build-b")
    ));
}

#[test]
fn persisted_state_rejects_two_staged_attempts() {
    let mut state = DeploymentState::default();
    stage(&mut state, "build-a");
    let mut json = serde_json::to_value(&state).unwrap();
    let mut second = json["generations"]["1"].clone();
    second["build"] = "build-b".into();
    json["generations"]["2"] = second;
    json["nextGeneration"] = 3.into();

    let invalid: DeploymentState = serde_json::from_value(json).unwrap();
    assert_eq!(
        invalid.validate(),
        Err(DeployError::InvalidPersistedState(
            "multiple staged generations"
        ))
    );
}

#[test]
fn explicit_retry_of_a_failed_build_allocates_a_fresh_generation() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "candidate failed".into(),
        })
        .unwrap();

    assert!(matches!(
        state.clone().plan_deploy(build("build-a")),
        Ok(DeployPlan::Unavailable { failed_target, .. }) if failed_target == failed
    ));
    let Some(DeployPlan::StageTarget {
        target,
        build: retried,
    }) = state.retry_deploy(build("build-a")).unwrap()
    else {
        panic!("explicit retry did not stage a target")
    };
    assert_ne!(target, failed);
    assert_eq!(retried, build("build-a"));
}

#[test]
fn explicit_retry_is_idempotent_after_the_fresh_attempt_is_staged() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "candidate failed".into(),
        })
        .unwrap();
    let first = state.retry_deploy(build("build-a")).unwrap();

    assert_eq!(state.retry_deploy(build("build-a")).unwrap(), first);
}

#[test]
fn retry_intent_waits_for_failed_activation_rollback_to_settle() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let failed = stage(&mut state, "build-b");
    state
        .reconcile(DeploymentEvent::Prepared { target: failed })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target: failed })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "candidate failed".into(),
        })
        .unwrap();

    assert_eq!(state.retry_deploy(build("build-b")).unwrap(), None);
    state
        .reconcile(DeploymentEvent::AdmissionsResumed {
            failed_target: failed,
        })
        .unwrap();
    let Some(DeployPlan::StageTarget { target, .. }) =
        state.retry_deploy(build("build-b")).unwrap()
    else {
        panic!("retry did not stage after rollback")
    };
    assert_ne!(target, failed);
    assert_eq!(state.snapshot().last_rollback, Some(failed));
    assert_eq!(
        state
            .snapshot()
            .generations
            .iter()
            .find(|item| item.id == previous)
            .unwrap()
            .status,
        GenerationStatus::Active
    );
}

#[test]
fn persisted_retry_receipt_prevents_reallocation_after_the_retry_later_fails() {
    let mut state = DeploymentState::default();
    let previous = activate_first(&mut state, "build-a");
    let first_failed = stage(&mut state, "build-b");
    complete_activation_failure(&mut state, first_failed);
    state
        .reconcile(DeploymentEvent::AdmissionsResumed {
            failed_target: first_failed,
        })
        .unwrap();
    let retry = RetryId::for_test(2);
    assert!(state
        .consume_retry(build("build-b"), retry.clone())
        .unwrap());
    let retried = state
        .snapshot()
        .generations
        .iter()
        .find(|generation| {
            generation.build == build("build-b")
                && generation.id != first_failed
                && generation.status == GenerationStatus::Staged
        })
        .unwrap()
        .id;

    let mut recovered: DeploymentState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    complete_activation_failure(&mut recovered, retried);
    recovered
        .reconcile(DeploymentEvent::AdmissionsResumed {
            failed_target: retried,
        })
        .unwrap();
    let attempts_before = recovered.snapshot().generations.len();

    assert!(recovered.consume_retry(build("build-b"), retry).unwrap());
    assert_eq!(recovered.snapshot().generations.len(), attempts_before);
    assert_eq!(
        recovered.current_plan().unwrap(),
        DeployPlan::Settled {
            active: Some(previous)
        }
    );
}

#[test]
fn retry_nonce_cannot_be_reused_for_another_build_or_corrupt_receipt() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "build-a");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "candidate failed".into(),
        })
        .unwrap();
    let retry = RetryId::for_test(3);
    assert!(state
        .consume_retry(build("build-a"), retry.clone())
        .unwrap());
    assert_eq!(
        state.consume_retry(build("build-b"), retry.clone()),
        Err(DeployError::InvalidTransition("retry id build mismatch"))
    );

    let mut corrupted = serde_json::to_value(state).unwrap();
    corrupted["retryReceipts"][retry.as_str()]["build"] = "build-b".into();
    let corrupted: DeploymentState = serde_json::from_value(corrupted).unwrap();
    assert_eq!(
        corrupted.validate(),
        Err(DeployError::InvalidPersistedState("invalid retry receipt"))
    );
}

fn complete_activation_failure(state: &mut DeploymentState, target: GenerationId) {
    state
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::PreviousPaused { target })
        .unwrap();
    state
        .reconcile(DeploymentEvent::Failed {
            generation: target,
            reason: "candidate failed".into(),
        })
        .unwrap();
}
