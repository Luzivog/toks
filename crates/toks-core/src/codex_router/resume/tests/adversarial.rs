use super::*;

#[test]
fn stale_selected_account_keeps_waiting_identity_and_launches_current_account() {
    let harness = Harness::new("stale-account");
    let original = harness.queue.0.borrow().waiting[0].clone();
    harness.queue.0.borrow_mut().stale_once_to = Some(AccountId::new("account-b"));

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    let attempt = &state.attempts[&original.thread_id];
    assert_eq!(attempt.account, AccountId::new("account-b"));
    assert_eq!(attempt.waiting.waiting_id, original.waiting_id);
    assert_eq!(harness.units.0.borrow().launches.len(), 1);
    assert_eq!(harness.queue.0.borrow().claims, [original.thread_id]);
}

#[test]
fn running_cancellation_stops_before_retiring_and_retries_stop_failure() {
    let harness = Harness::new("running-cancel");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Running);
    harness.units.0.borrow_mut().fail_cancels = 1;
    let mut settings = harness.settings.load().unwrap();
    settings.cancel_thread(&ThreadId::new("running-cancel"));
    harness.settings.save(&settings).unwrap();

    assert!(harness.supervisor().tick(NOW).is_err());
    assert_eq!(harness.attempt(), attempt);
    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(harness.units.0.borrow().cleanups, [attempt]);
    assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
}

#[test]
fn failed_unit_before_wrapper_receipt_is_not_relaunched() {
    let harness = Harness::new("pre-receipt-failure");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Failed);

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(harness.units.0.borrow().launches.len(), 1);
    assert_eq!(harness.units.0.borrow().cleanups, [attempt]);
}

#[test]
fn invalid_workspace_head_gets_its_own_delay_and_does_not_starve_next() {
    let harness = Harness::new("missing-workspace");
    let blocked = harness.queue.0.borrow().waiting[0].clone();
    let ready = WaitingThread::new(ThreadId::new("ready"), NOW);
    harness.queue.0.borrow_mut().waiting.push(ready.clone());
    let workspace = harness.workspace.clone();
    let mut supervisor = harness.supervisor_with_workspace(move |thread| {
        if thread.as_str() == "missing-workspace" {
            anyhow::bail!("workspace missing")
        }
        Ok(workspace.clone())
    });

    supervisor.tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert!(state.retry_after.contains_key(&blocked.waiting_id));
    assert_eq!(
        state.attempts[&ready.thread_id].waiting.waiting_id,
        ready.waiting_id
    );
    assert_eq!(harness.units.0.borrow().launches[0].1, ready.thread_id);
}

#[test]
fn unavailable_owned_head_does_not_starve_ready_task_and_keeps_its_affinity() {
    let harness = Harness::new("owned-head");
    let head = harness.queue.0.borrow().waiting[0].clone();
    let ready = WaitingThread::new(ThreadId::new("ready-behind-owned"), NOW);
    {
        let mut queue = harness.queue.0.borrow_mut();
        queue.waiting.push(ready.clone());
        queue
            .eligible_by_thread
            .insert(head.thread_id.clone(), None);
    }

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&ready.thread_id].waiting.waiting_id,
        ready.waiting_id
    );
    assert_eq!(harness.units.0.borrow().launches[0].1, ready.thread_id);
    assert_eq!(harness.queue.0.borrow().waiting, vec![head.clone()]);

    let account_b = AccountId::new("account-b");
    harness
        .queue
        .0
        .borrow_mut()
        .eligible_by_thread
        .insert(head.thread_id.clone(), Some(account_b.clone()));
    harness
        .supervisor()
        .tick(UnixMillis::new(NOW.get() + 1))
        .unwrap();

    let state = harness.store.load().unwrap();
    assert_eq!(state.attempts[&head.thread_id].account, account_b);
}

#[test]
fn persistently_stale_head_is_bounded_and_does_not_starve_next_task() {
    let harness = Harness::new("stale-head");
    let head = harness.queue.0.borrow().waiting[0].clone();
    let ready = WaitingThread::new(ThreadId::new("ready-after-stale"), NOW);
    {
        let mut queue = harness.queue.0.borrow_mut();
        queue.waiting.push(ready.clone());
        queue.stale_threads.insert(head.thread_id.clone());
    }

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&ready.thread_id].waiting.waiting_id,
        ready.waiting_id
    );
    assert_eq!(
        harness.queue.0.borrow().authorization_calls,
        [head.thread_id, ready.thread_id]
    );
}

#[test]
fn same_millisecond_reenqueue_cannot_be_claimed_by_old_success() {
    let harness = Harness::new("aba");
    let original = harness.queue.0.borrow().waiting[0].clone();
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    let replacement = WaitingThread::new(ThreadId::new("aba"), NOW);
    assert_eq!(replacement.since, original.since);
    assert_ne!(replacement.waiting_id, original.waiting_id);
    harness.queue.0.borrow_mut().waiting = vec![replacement.clone()];
    harness.store.record_outcome(&attempt, true).unwrap();

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&replacement.thread_id].waiting.waiting_id,
        replacement.waiting_id
    );
}

#[test]
fn stale_retry_delay_does_not_block_a_new_waiting_identity() {
    let harness = Harness::new("retry-aba");
    let original = harness.queue.0.borrow().waiting[0].clone();
    let replacement = WaitingThread::new(original.thread_id.clone(), original.since);
    let mut state = harness.store.load().unwrap();
    state
        .retry_after
        .insert(original.waiting_id, UnixMillis::new(NOW.get() + 60_000));
    harness.store.save(&state).unwrap();
    harness.queue.0.borrow_mut().waiting = vec![replacement.clone()];

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&replacement.thread_id].waiting.waiting_id,
        replacement.waiting_id
    );
}

#[test]
fn crash_after_runtime_requeue_reuses_the_planned_waiting_identity() {
    let harness = Harness::new("requeue-crash");
    harness.supervisor().tick(NOW).unwrap();
    let attempt_id = harness.attempt();
    let attempt = harness.store.load().unwrap().attempts[&ThreadId::new("requeue-crash")].clone();
    harness.queue.0.borrow_mut().waiting.clear();
    let replacement = harness
        .queue
        .clone()
        .finish(
            &attempt.waiting,
            &attempt.id,
            ResumeTerminal::Failure,
            attempt.retry_waiting_id.clone(),
        )
        .unwrap()
        .unwrap();
    harness.store.record_outcome(&attempt_id, false).unwrap();

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert!(state.attempts.is_empty());
    assert!(state.retry_after.contains_key(&replacement.waiting_id));
    assert_eq!(harness.units.0.borrow().launches.len(), 1);
}

#[test]
fn crash_after_automatic_authorization_recovers_the_same_attempt() {
    let harness = Harness::new("authorization-crash");
    harness.queue.0.borrow_mut().crash_after_authorize = 1;

    assert!(harness.supervisor().tick(NOW).is_err());
    let attempt = harness.attempt();
    assert!(harness.units.0.borrow().launches.is_empty());
    assert!(harness.queue.0.borrow().waiting.is_empty());

    harness.supervisor().tick(NOW).unwrap();

    assert_eq!(harness.attempt(), attempt);
    assert_eq!(harness.units.0.borrow().launches.len(), 1);
    assert_eq!(harness.units.0.borrow().launches[0].0, attempt);
}

#[test]
fn cancellation_after_authorization_crash_restores_waiting_without_launch() {
    let harness = Harness::new("cancel-authorizing");
    harness.queue.0.borrow_mut().crash_after_authorize = 1;
    assert!(harness.supervisor().tick(NOW).is_err());
    let mut settings = harness.settings.load().unwrap();
    settings.cancel_thread(&ThreadId::new("cancel-authorizing"));
    harness.settings.save(&settings).unwrap();

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
    assert!(harness.units.0.borrow().launches.is_empty());
    assert_eq!(harness.units.0.borrow().cleanups.len(), 1);
}

#[test]
fn manual_claim_after_snapshot_but_before_authorization_prevents_auto_launch() {
    let harness = Harness::new("manual-race");
    harness.queue.0.borrow_mut().fail_authorizations = 1;
    assert!(harness.supervisor().tick(NOW).is_err());
    harness.queue.0.borrow_mut().waiting.clear();

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert!(harness.units.0.borrow().launches.is_empty());
    assert!(harness.units.0.borrow().cleanups.is_empty());
}

#[test]
fn lost_authorization_does_not_touch_another_attempts_admission() {
    let harness = Harness::new("lost-to-other-attempt");
    let waiting = harness.queue.0.borrow().waiting[0].clone();
    harness.queue.0.borrow_mut().fail_authorizations = 1;
    assert!(harness.supervisor().tick(NOW).is_err());
    {
        let mut queue = harness.queue.0.borrow_mut();
        queue.waiting.clear();
        queue.admissions.insert(
            waiting.waiting_id.clone(),
            FakeAdmission {
                attempt: "different-attempt".into(),
                finished: None,
            },
        );
    }

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(
        harness.queue.0.borrow().admissions[&waiting.waiting_id].attempt,
        "different-attempt"
    );
    assert!(harness.units.0.borrow().launches.is_empty());
}

#[test]
fn failed_cleanup_keeps_terminal_attempt_and_converges_on_retry() {
    let harness = Harness::new("cleanup-retry");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.store.record_outcome(&attempt, true).unwrap();
    harness.units.0.borrow_mut().fail_cleanups = 1;

    assert!(harness.supervisor().tick(NOW).is_err());
    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&ThreadId::new("cleanup-retry")].phase,
        ResumePhase::Cleaning
    );
    assert!(harness.store.outcome(&attempt).unwrap().is_some());

    harness.supervisor().tick(NOW).unwrap();
    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert!(harness.store.outcome(&attempt).unwrap().is_none());
    assert!(harness.queue.0.borrow().admissions.is_empty());
}

#[test]
fn permanently_poisoned_cleanup_does_not_block_other_progress() {
    let harness = Harness::new("poisoned-cleanup");
    harness.supervisor().tick(NOW).unwrap();
    let poisoned = harness.attempt();
    let mut state = harness.store.load().unwrap();
    let original = state
        .attempts
        .get_mut(&ThreadId::new("poisoned-cleanup"))
        .unwrap();
    original.phase = ResumePhase::Cleaning;
    original.terminal = Some(crate::codex_router::resume::state::ResumeTerminalState::Success);
    let mut second = original.clone();
    second.id = uuid::Uuid::new_v4().to_string();
    second.retry_waiting_id = WaitingId::for_attempt(&second.id);
    second.waiting = WaitingThread::new(ThreadId::new("poisoned-cleanup-2"), NOW);
    let second_id = second.id.clone();
    state
        .attempts
        .insert(second.waiting.thread_id.clone(), second);
    harness.store.save(&state).unwrap();
    harness.units.0.borrow_mut().permanent_cleanup_failures =
        [poisoned.clone(), second_id].into_iter().collect();
    let ready = WaitingThread::new(ThreadId::new("ready-after-poison"), NOW);
    harness.queue.0.borrow_mut().waiting.push(ready.clone());

    let error = harness.supervisor().tick(NOW).unwrap_err();
    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&ThreadId::new("poisoned-cleanup")].phase,
        ResumePhase::Cleaning
    );
    assert!(state.attempts.contains_key(&ready.thread_id));
    let message = format!("{error:#}");
    assert!(message.contains("poisoned-cleanup"));
    assert!(message.contains("poisoned-cleanup-2"));
    assert!(message.contains("synthetic permanent cleanup failure"));
    let ready_attempt = state.attempts[&ready.thread_id].id.clone();
    harness.store.record_outcome(&ready_attempt, true).unwrap();
    let tail = WaitingThread::new(ThreadId::new("tail-after-poison"), NOW);
    harness.queue.0.borrow_mut().waiting.push(tail.clone());

    let error = harness.supervisor().tick(NOW).unwrap_err();
    let state = harness.store.load().unwrap();
    assert!(!state.attempts.contains_key(&ready.thread_id));
    assert!(state.attempts.contains_key(&tail.thread_id));
    assert!(format!("{error:#}").contains("poisoned-cleanup"));
}

#[test]
fn failure_tombstone_stays_cleaning_until_forget_converges() {
    let harness = Harness::new("forget-retry");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.queue.0.borrow_mut().waiting.clear();
    harness.store.record_outcome(&attempt, false).unwrap();
    harness.queue.0.borrow_mut().fail_forgets = 1;

    assert!(harness.supervisor().tick(NOW).is_err());
    let state = harness.store.load().unwrap();
    let attempt_state = &state.attempts[&ThreadId::new("forget-retry")];
    assert_eq!(attempt_state.phase, ResumePhase::Cleaning);
    assert_eq!(
        attempt_state.terminal,
        Some(crate::codex_router::resume::state::ResumeTerminalState::Failure)
    );
    assert_eq!(harness.queue.0.borrow().admissions.len(), 1);
    assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
    assert_eq!(harness.queue.0.borrow().requeues.len(), 1);

    harness.supervisor().tick(NOW).unwrap();
    assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
    assert_eq!(harness.queue.0.borrow().requeues.len(), 1);
    assert!(harness.queue.0.borrow().admissions.is_empty());
    assert!(harness.store.load().unwrap().attempts.is_empty());
}

#[test]
fn cancelled_tombstone_stays_cleaning_until_forget_converges() {
    let harness = Harness::new("cancel-forget-retry");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Running);
    let mut settings = harness.settings.load().unwrap();
    settings.cancel_thread(&ThreadId::new("cancel-forget-retry"));
    harness.settings.save(&settings).unwrap();
    harness.queue.0.borrow_mut().fail_forgets = 1;

    assert!(harness.supervisor().tick(NOW).is_err());
    let state = harness.store.load().unwrap();
    let attempt_state = &state.attempts[&ThreadId::new("cancel-forget-retry")];
    assert_eq!(attempt_state.phase, ResumePhase::Cleaning);
    assert_eq!(
        attempt_state.terminal,
        Some(crate::codex_router::resume::state::ResumeTerminalState::Cancelled)
    );
    assert_eq!(harness.queue.0.borrow().admissions.len(), 1);
    assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
    assert_eq!(harness.queue.0.borrow().requeues.len(), 1);

    harness.supervisor().tick(NOW).unwrap();
    assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
    assert_eq!(harness.queue.0.borrow().requeues.len(), 1);
    assert!(harness.queue.0.borrow().admissions.is_empty());
    assert!(harness.store.load().unwrap().attempts.is_empty());
}

#[test]
fn many_attempts_use_one_bounded_inventory_call_per_tick() {
    let harness = Harness::new("batch-0");
    for index in 1..100 {
        harness
            .queue
            .0
            .borrow_mut()
            .waiting
            .push(WaitingThread::new(
                ThreadId::new(format!("batch-{index}")),
                NOW,
            ));
    }
    let mut supervisor = harness.supervisor();
    for _ in 0..100 {
        supervisor.tick(NOW).unwrap();
    }
    assert_eq!(harness.store.load().unwrap().attempts.len(), 100);
    {
        let mut units = harness.units.0.borrow_mut();
        units.inventory_calls = 0;
        units.fail_inventory = true;
    }

    assert!(supervisor.tick(NOW).is_err());
    assert_eq!(harness.units.0.borrow().inventory_calls, 1);
}
