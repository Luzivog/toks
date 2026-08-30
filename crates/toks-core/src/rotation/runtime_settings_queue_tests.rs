use crate::accounts::AccountId;
use crate::storage::StoreUpdate;
use std::sync::mpsc;
use std::time::Duration;

use super::{
    ResumeAuthorization, ResumeTerminal, RotationRuntime, RotationSettings, RotationSettingsStore,
    ThreadId, ThreadOverrideChange, UnixMillis, WaitingId,
};

const ACTIVE_RESUME_ATTEMPT: &str = "00000000-0000-4000-8000-000000000010";

#[test]
fn active_resume_remains_logically_queued_for_a_concurrent_cancellation() {
    let account = AccountId::new("account");
    let thread = ThreadId::new("active-resume");
    let mut runtime = RotationRuntime::default();
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.waiting(&thread, UnixMillis::new(1));
    let waiting = runtime.waiting_threads()[0].clone();
    settings.reconcile_thread_state(&runtime);
    assert_eq!(
        runtime.authorize_resume(
            &settings,
            std::slice::from_ref(&account),
            &waiting,
            ACTIVE_RESUME_ATTEMPT,
            &account,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Acquired
    );
    assert_eq!(
        runtime.queued_or_resuming_threads(),
        std::slice::from_ref(&thread)
    );

    settings.cancel_thread(&thread);
    settings.reconcile_thread_state(&runtime);

    assert!(settings.cancelled_threads().contains(&thread));
}

#[test]
fn active_then_failed_resume_keeps_its_existing_queue_priority() {
    let account = AccountId::new("account");
    let first = ThreadId::new("first");
    let second = ThreadId::new("second");
    let mut runtime = RotationRuntime::default();
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.waiting(&first, UnixMillis::new(1));
    runtime.waiting(&second, UnixMillis::new(2));
    settings.reconcile_thread_state(&runtime);
    let waiting = runtime.waiting_threads()[0].clone();
    assert_eq!(
        settings.waiting_priority(),
        &[first.clone(), second.clone()]
    );
    assert_eq!(
        runtime.authorize_resume(
            &settings,
            std::slice::from_ref(&account),
            &waiting,
            ACTIVE_RESUME_ATTEMPT,
            &account,
            UnixMillis::new(3),
        ),
        ResumeAuthorization::Acquired
    );

    settings.reconcile_thread_state(&runtime);
    assert_eq!(
        settings.waiting_priority(),
        &[first.clone(), second.clone()]
    );
    runtime.finish_resume(
        &waiting,
        ACTIVE_RESUME_ATTEMPT,
        ResumeTerminal::Failure,
        WaitingId::for_attempt(ACTIVE_RESUME_ATTEMPT),
        UnixMillis::new(4),
    );
    settings.reconcile_thread_state(&runtime);

    assert_eq!(settings.waiting_priority(), &[first, second]);
}

#[test]
fn active_cancellation_survives_polling_without_entering_the_waiting_order() {
    let account = AccountId::new("account");
    let thread = ThreadId::new("active-follow-up");
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(1))
        .unwrap();
    assert!(runtime.connection_continues(&account, &thread, UnixMillis::new(2)));
    let mut settings = RotationSettings::default();
    settings.cancel_thread(&thread);
    settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ServiceTier(Some("priority".into())),
        )
        .unwrap();

    settings.reconcile_thread_state(&runtime);

    assert!(settings.cancelled_threads().contains(&thread));
    assert!(settings.waiting_priority().is_empty());
    assert!(settings.thread_override(&thread).is_some());

    settings.reconcile_thread_state(&RotationRuntime::default());

    assert!(!settings.cancelled_threads().contains(&thread));
    assert!(settings.thread_override(&thread).is_none());
}

#[test]
fn stale_poll_and_cancel_transactions_cannot_overwrite_each_other() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationSettingsStore::for_data_dir(directory.path());
    let thread = ThreadId::new("concurrent-cancel");
    store.save(&RotationSettings::default()).unwrap();
    let polling = store.clone();
    let cancelling = store.clone();
    let polled_thread = thread.clone();
    let cancelled_thread = thread.clone();
    let (poll_loaded_tx, poll_loaded_rx) = mpsc::channel();
    let (release_poll_tx, release_poll_rx) = mpsc::channel();
    let (cancel_started_tx, cancel_started_rx) = mpsc::channel();
    let (cancel_done_tx, cancel_done_rx) = mpsc::channel();

    let poll = std::thread::spawn(move || {
        polling
            .update(|settings| {
                poll_loaded_tx.send(()).unwrap();
                release_poll_rx.recv().unwrap();
                let changed = settings.reconcile_threads(&[polled_thread], &[]);
                StoreUpdate::from_changed((), changed)
            })
            .unwrap();
    });
    poll_loaded_rx.recv().unwrap();
    let cancel = std::thread::spawn(move || {
        cancel_started_tx.send(()).unwrap();
        cancelling
            .update(|settings| {
                let changed = settings.cancel_thread(&cancelled_thread);
                StoreUpdate::from_changed((), changed)
            })
            .unwrap();
        cancel_done_tx.send(()).unwrap();
    });
    cancel_started_rx.recv().unwrap();
    assert!(matches!(
        cancel_done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    release_poll_tx.send(()).unwrap();
    poll.join().unwrap();
    cancel.join().unwrap();
    cancel_done_rx.recv().unwrap();

    assert!(store.load().unwrap().cancelled_threads().contains(&thread));
}
