use std::sync::mpsc;
use std::time::Duration;

use crate::accounts::AccountId;
use crate::rotation::{ResumeAuthorization, RotationSettingsStore, ThreadId};
use crate::storage::StoreUpdate;

use super::{Engines, ATTEMPT};

#[test]
fn committed_cancellation_wins_before_resume_authorization() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("cancel-during-authorization");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();
    let store = RotationSettingsStore::for_data_dir(engines._directory.path());
    let held_store = store.clone();
    let cancelled_thread = thread.clone();
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let holding = std::thread::spawn(move || {
        held_store
            .update(|settings| {
                held_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                let changed = settings.cancel_thread(&cancelled_thread);
                StoreUpdate::from_changed((), changed)
            })
            .unwrap();
    });
    held_rx.recv().unwrap();

    let authorizing = engines.second.clone();
    let authorized_waiting = waiting.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let authorization = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let outcome = authorizing
            .authorize_resume(&authorized_waiting, ATTEMPT, &account)
            .unwrap();
        done_tx.send(()).unwrap();
        outcome
    });
    started_rx.recv().unwrap();
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    release_tx.send(()).unwrap();
    holding.join().unwrap();
    assert_eq!(
        authorization.join().unwrap(),
        ResumeAuthorization::Cancelled
    );
    done_rx.recv().unwrap();
    assert_eq!(
        engines.store.load().unwrap().waiting_threads(),
        std::slice::from_ref(&waiting)
    );
}

#[test]
fn committed_exclusion_wins_before_route_opens_a_connection() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("exclude-during-route");
    let store = RotationSettingsStore::for_data_dir(engines._directory.path());
    let held_store = store.clone();
    let excluded_account = account.clone();
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let holding = std::thread::spawn(move || {
        held_store
            .update(|settings| {
                held_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                let changed = settings.set_included(&excluded_account, false);
                StoreUpdate::from_changed((), changed)
            })
            .unwrap();
    });
    held_rx.recv().unwrap();

    let routing = engines.second.clone();
    let routed_account = account.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let route = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let outcome = routing.route(&routed_account, &thread).unwrap();
        done_tx.send(()).unwrap();
        outcome
    });
    started_rx.recv().unwrap();
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    release_tx.send(()).unwrap();
    holding.join().unwrap();
    assert_eq!(route.join().unwrap(), None);
    done_rx.recv().unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 0);
}
