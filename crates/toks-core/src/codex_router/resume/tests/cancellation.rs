use super::*;

#[test]
fn cancellation_after_candidate_enumeration_stops_the_stale_tick() {
    let harness = Harness::new("cancel-after-enumeration");
    let cancelled = harness.queue.0.borrow().waiting[0].clone();
    let ready = WaitingThread::new(ThreadId::new("ready-behind-cancelled"), NOW);
    {
        let mut queue = harness.queue.0.borrow_mut();
        queue.waiting.push(ready.clone());
        queue.cancel_on_authorization_call = Some(1);
    }

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert!(harness.units.0.borrow().launches.is_empty());
    assert_eq!(
        harness.queue.0.borrow().waiting,
        vec![cancelled.clone(), ready]
    );
    assert_eq!(
        harness.queue.0.borrow().authorization_calls,
        [cancelled.thread_id]
    );
}

#[test]
fn cancellation_after_stale_account_retry_stops_the_stale_tick() {
    let harness = Harness::new("cancel-after-stale-account");
    let cancelled = harness.queue.0.borrow().waiting[0].clone();
    let ready = WaitingThread::new(ThreadId::new("ready-behind-stale-cancel"), NOW);
    {
        let mut queue = harness.queue.0.borrow_mut();
        queue.waiting.push(ready.clone());
        queue.stale_once_to = Some(AccountId::new("account-b"));
        queue.cancel_on_authorization_call = Some(2);
    }

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert!(harness.units.0.borrow().launches.is_empty());
    assert_eq!(
        harness.queue.0.borrow().waiting,
        vec![cancelled.clone(), ready]
    );
    assert_eq!(
        harness.queue.0.borrow().authorization_calls,
        [cancelled.thread_id.clone(), cancelled.thread_id]
    );
}

#[test]
fn cancellation_committed_after_authorization_prevents_launch() {
    let harness = Harness::new("cancel-after-authorization");
    let thread = ThreadId::new("cancel-after-authorization");
    let waiting = harness.queue.0.borrow().waiting[0].clone();
    let id = uuid::Uuid::new_v4().to_string();
    let attempt = super::super::state::ResumeAttempt {
        retry_waiting_id: WaitingId::for_attempt(&id),
        id,
        account: AccountId::new("account"),
        waiting,
        cwd: harness.workspace.clone(),
        phase: ResumePhase::Authorizing,
        terminal: None,
    };
    let mut state = super::super::state::ResumeState::default();
    state.attempts.insert(thread.clone(), attempt.clone());
    harness.store.save(&state).unwrap();
    harness
        .settings
        .update(|settings| ((), settings.cancel_waiting(&thread)))
        .unwrap();

    assert_eq!(
        harness
            .supervisor()
            .authorize(&mut state, &thread, &attempt, NOW)
            .unwrap(),
        super::super::supervisor::AuthorizationOutcome::Cancelled
    );

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert!(harness.units.0.borrow().launches.is_empty());
    assert_eq!(
        harness.queue.0.borrow().claims,
        std::slice::from_ref(&thread)
    );
    assert_eq!(harness.queue.0.borrow().waiting[0].thread_id, thread);
}
