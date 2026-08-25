use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::accounts::AccountId;
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{
    QuotaObservation, ResumeAuthorization, RotationRuntimeStore, RotationSettings,
    RotationSettingsStore, ThreadId, UnixMillis, WorkerConnectionOwner,
};
use crate::storage::StoreUpdate;

use super::{Engine, EngineConfig};
use crate::codex_router::proxy::catalogue::Catalogue;
use crate::codex_router::proxy::types::{
    CredentialFailure, CredentialSource, RouteCredential, SharedCredentials,
};

mod auth_repair;
mod cross_account;
mod quota_reset_tests;
mod quota_snapshot_tests;
mod reconciliation;
mod resume_authorization;
mod settings_linearization;
mod thread_source;

const ATTEMPT: &str = "00000000-0000-4000-8000-000000000001";
const WRONG_ATTEMPT: &str = "00000000-0000-4000-8000-000000000002";

struct Credentials {
    accounts: Vec<AccountId>,
}

impl CredentialSource for Credentials {
    fn account_ids(&self) -> Vec<AccountId> {
        self.accounts.clone()
    }

    fn incoming_account(&self, _token: &str) -> Option<AccountId> {
        None
    }

    fn credential<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move { Ok(credential(account)) })
    }

    fn refresh<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move { Ok(credential(account)) })
    }
}

struct Engines {
    _directory: tempfile::TempDir,
    accounts: Vec<AccountId>,
    store: RotationRuntimeStore,
    first: Arc<Engine>,
    second: Arc<Engine>,
}

impl Engines {
    fn new() -> Self {
        Self::with_accounts(&["a"])
    }

    fn with_accounts(ids: &[&str]) -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let accounts = ids.iter().map(|id| AccountId::new(*id)).collect::<Vec<_>>();
        let settings_store = RotationSettingsStore::for_data_dir(directory.path());
        let mut settings = RotationSettings::default();
        settings.reconcile(&accounts);
        settings.set_enabled(true);
        settings_store.save(&settings).expect("save settings");
        let store = RotationRuntimeStore::for_data_dir(directory.path());
        let credentials: SharedCredentials = Arc::new(Credentials {
            accounts: accounts.clone(),
        });
        let build = || {
            Engine::new(EngineConfig {
                credentials: credentials.clone(),
                settings: settings_store.clone(),
                runtime_store: store.clone(),
                catalogue: Catalogue::at(None),
                connection_owner: None,
                thread_sources: ThreadSourceStore::discover(),
            })
            .expect("engine")
        };
        let first = build();
        let second = build();
        Self {
            _directory: directory,
            accounts,
            store,
            first,
            second,
        }
    }

    fn worker(&self, generation: u64, instance_id: u64) -> Arc<Engine> {
        let credentials: SharedCredentials = Arc::new(Credentials {
            accounts: self.accounts.clone(),
        });
        Engine::new(EngineConfig {
            credentials,
            settings: RotationSettingsStore::for_data_dir(self._directory.path()),
            runtime_store: self.store.clone(),
            catalogue: Catalogue::at(None),
            connection_owner: Some(
                WorkerConnectionOwner::new(generation, instance_id)
                    .expect("worker identity is nonzero"),
            ),
            thread_sources: ThreadSourceStore::discover(),
        })
        .expect("worker engine")
    }

    fn prioritize(&self, account: &AccountId) {
        let store = RotationSettingsStore::for_data_dir(self._directory.path());
        let mut settings = store.load().unwrap();
        assert!(settings.move_to(account, 0));
        store.save(&settings).unwrap();
    }

    fn cancel(&self, thread: &ThreadId) {
        let store = RotationSettingsStore::for_data_dir(self._directory.path());
        let mut settings = store.load().unwrap();
        assert!(settings.cancel_waiting(thread));
        store.save(&settings).unwrap();
    }
}

#[test]
fn separate_engines_do_not_overwrite_each_others_runtime_updates() {
    let engines = Engines::new();
    engines.first.waiting(&ThreadId::new("first")).unwrap();
    engines.second.waiting(&ThreadId::new("second")).unwrap();

    let runtime = engines.store.load().unwrap();
    let waiting = runtime
        .waiting_threads()
        .iter()
        .map(|entry| entry.thread_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(waiting, ["first", "second"]);
}

#[test]
fn a_waiting_thread_can_only_be_claimed_once_across_engines() {
    let engines = Engines::new();
    let thread = ThreadId::new("waiting");
    let account = AccountId::new("a");
    engines.first.waiting(&thread).unwrap();

    assert!(engines.second.claim_waiting(&thread, &account).unwrap());
    assert!(!engines.first.claim_waiting(&thread, &account).unwrap());
}

#[test]
fn manual_route_wins_before_automatic_authorization() {
    let engines = Engines::new();
    let thread = ThreadId::new("manual-first");
    let account = AccountId::new("a");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();

    assert!(engines.first.route(&account, &thread).unwrap().is_some());
    assert_eq!(
        engines
            .second
            .authorize_resume(&waiting, ATTEMPT, &account)
            .unwrap(),
        ResumeAuthorization::Lost
    );
}

#[test]
fn automatic_authorization_blocks_manual_route_without_exact_marker() {
    let engines = Engines::new();
    let thread = ThreadId::new("automatic-first");
    let account = AccountId::new("a");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();

    assert_eq!(
        engines
            .first
            .authorize_resume(&waiting, ATTEMPT, &account)
            .unwrap(),
        ResumeAuthorization::Acquired
    );
    assert!(engines.first.route(&account, &thread).unwrap().is_none());
    assert!(engines
        .first
        .route_authorized(&account, &thread, Some(WRONG_ATTEMPT))
        .unwrap()
        .is_none());
    assert!(engines
        .first
        .route_authorized(&account, &thread, Some(ATTEMPT))
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn automatic_marker_selects_its_bound_account_after_priority_changes() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let thread = ThreadId::new("bound-account");
    let account = AccountId::new("a");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();
    engines
        .first
        .authorize_resume(&waiting, ATTEMPT, &account)
        .unwrap();
    engines.prioritize(&AccountId::new("b"));

    let selected = engines
        .second
        .select_for_thread_authorized(
            Some(&thread),
            crate::codex_router::proxy::headers::ResumeMarker::Canonical(ATTEMPT),
            &Default::default(),
        )
        .await
        .unwrap();
    let super::selection::RouteSelection::Selected(selected) = selected else {
        panic!("expected selected credential");
    };
    assert_eq!(selected.account_id, account);
    assert!(engines
        .second
        .select_for_thread(Some(&thread), &Default::default())
        .await
        .unwrap()
        .is_none());
}

#[test]
fn denied_manual_route_cannot_create_a_second_waiting_identity() {
    let engines = Engines::new();
    let thread = ThreadId::new("no-second-wait");
    let account = AccountId::new("a");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();
    engines
        .first
        .authorize_resume(&waiting, ATTEMPT, &account)
        .unwrap();

    assert!(engines.first.route(&account, &thread).unwrap().is_none());
    engines.first.waiting(&thread).unwrap();
    assert!(engines.store.load().unwrap().waiting_threads().is_empty());
    engines
        .first
        .finish_resume(
            &waiting,
            ATTEMPT,
            crate::rotation::ResumeTerminal::Success,
            crate::rotation::WaitingId::for_test("unused"),
        )
        .unwrap();
    assert!(engines.store.load().unwrap().waiting_threads().is_empty());
    engines.second.forget_resume(&waiting, ATTEMPT).unwrap();
    engines.second.waiting(&thread).unwrap();
    assert_eq!(engines.store.load().unwrap().waiting_threads().len(), 1);
}

#[test]
fn overlapping_engines_serialize_the_whole_read_modify_write() {
    use std::sync::mpsc;
    use std::time::Duration;

    let engines = Engines::new();
    let first = engines.first.clone();
    let second = engines.second.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let holding = std::thread::spawn(move || {
        first
            .runtime
            .update(|runtime| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                StoreUpdate::from_changed(
                    (),
                    runtime.waiting(&ThreadId::new("first"), UnixMillis::now()),
                )
            })
            .unwrap();
    });
    entered_rx.recv().unwrap();

    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let waiting = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        second.waiting(&ThreadId::new("second")).unwrap();
        done_tx.send(()).unwrap();
    });
    started_rx.recv().unwrap();
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    release_tx.send(()).unwrap();
    holding.join().unwrap();
    waiting.join().unwrap();
    done_rx.recv().unwrap();
    assert_eq!(engines.store.load().unwrap().waiting_threads().len(), 2);
}

#[tokio::test]
async fn concurrent_cross_engine_selections_keep_one_reservation_each() {
    let engines = Engines::new();
    let thread = ThreadId::new("same-thread");
    let skipped = Default::default();
    let (first, second) = tokio::join!(
        engines.first.select_for_thread(Some(&thread), &skipped),
        engines.second.select_for_thread(Some(&thread), &skipped),
    );
    first.unwrap();
    second.unwrap();

    let account = AccountId::new("a");
    assert!(engines.first.route(&account, &thread).unwrap().is_some());
    engines.first.close(&account, &thread).unwrap();
    assert_eq!(engines.store.load().unwrap().active_threads(&account), 1);
    assert!(engines.second.route(&account, &thread).unwrap().is_some());
    engines.second.close(&account, &thread).unwrap();
    assert_eq!(engines.store.load().unwrap().active_threads(&account), 0);
}

#[test]
fn cold_reconciliation_preserves_only_surviving_worker_connections() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let dead_stream = ThreadId::new("dead-stream");
    let follow_up = ThreadId::new("dead-worker-follow-up");
    let surviving_stream = ThreadId::new("surviving-stream");
    let dead_attachment = ThreadId::new("dead-attachment");
    let surviving_attachment = ThreadId::new("surviving-attachment");
    let first = engines.worker(41, 4_101);
    let second = engines.worker(42, 4_201);

    assert!(first.route(&account, &dead_stream).unwrap().is_some());
    assert!(first.route(&account, &follow_up).unwrap().is_some());
    first.continue_response(&account, &follow_up).unwrap();
    assert!(first.attach(&account, &dead_attachment).unwrap());
    assert!(second.route(&account, &surviving_stream).unwrap().is_some());
    assert!(second.attach(&account, &surviving_attachment).unwrap());

    engines
        .first
        .reconcile_connection_owners(&std::collections::BTreeMap::from([(42, 4_201)]))
        .unwrap();
    engines
        .first
        .runtime
        .update(|runtime| {
            let changed = runtime.apply_quota_observations(
                &std::collections::BTreeMap::from([(
                    account.clone(),
                    QuotaObservation::Draining(Some(UnixMillis::new(i64::MAX))),
                )]),
                UnixMillis::new(10),
            );
            StoreUpdate::from_changed((), changed)
        })
        .unwrap();

    let runtime = engines.store.load().unwrap();
    assert_eq!(runtime.active_threads(&account), 1);
    assert!(!runtime.can_drain(&account, &dead_stream, UnixMillis::new(11)));
    assert!(runtime.can_drain(&account, &follow_up, UnixMillis::new(11)));
    assert!(runtime.can_drain(&account, &surviving_stream, UnixMillis::new(11)));
    assert!(!runtime.can_drain(&account, &dead_attachment, UnixMillis::new(11)));
    assert!(runtime.can_drain(&account, &surviving_attachment, UnixMillis::new(11)));
}

#[test]
fn restarting_one_generation_cannot_inherit_or_close_its_predecessors_connections() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let stale = ThreadId::new("stale-process-stream");
    let current = ThreadId::new("current-process-stream");
    let predecessor = engines.worker(7, 701);
    assert!(predecessor.route(&account, &stale).unwrap().is_some());

    let replacement = engines.worker(7, 702);
    assert_eq!(engines.store.load().unwrap().active_threads(&account), 0);
    assert!(replacement.route(&account, &current).unwrap().is_some());
    predecessor.close(&account, &current).unwrap();
    assert_eq!(engines.store.load().unwrap().active_threads(&account), 1);

    replacement.close(&account, &current).unwrap();
    assert_eq!(engines.store.load().unwrap().active_threads(&account), 0);
}

#[test]
fn starting_another_engine_preserves_live_connections() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("live-thread");
    assert!(engines.first.route(&account, &thread).unwrap().is_some());

    let credentials: SharedCredentials = Arc::new(Credentials {
        accounts: vec![account.clone()],
    });
    let _third = Engine::new(EngineConfig {
        credentials,
        settings: RotationSettingsStore::for_data_dir(engines._directory.path()),
        runtime_store: engines.store.clone(),
        catalogue: Catalogue::at(None),
        connection_owner: None,
        thread_sources: ThreadSourceStore::discover(),
    })
    .unwrap();

    assert_eq!(engines.store.load().unwrap().active_threads(&account), 1);
}

#[test]
fn explicit_cold_start_reset_clears_only_live_process_ownership() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("live-before-cold-start");
    engines
        .first
        .runtime
        .update(|runtime| {
            runtime
                .connection_opened(&account, &thread, UnixMillis::new(10))
                .unwrap();
            StoreUpdate::Changed(())
        })
        .unwrap();

    engines.first.reset_connections().unwrap();

    assert_eq!(engines.store.load().unwrap().active_threads(&account), 0);
}

#[test]
fn quota_updates_in_a_new_generation_see_old_worker_attachments() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("attached-in-old-worker");
    assert!(engines.first.attach(&account, &thread).unwrap());

    engines
        .second
        .runtime
        .update(|runtime| {
            let changed = runtime.apply_quota_observations(
                &std::collections::BTreeMap::from([(
                    account.clone(),
                    QuotaObservation::Draining(Some(crate::rotation::UnixMillis::new(100))),
                )]),
                crate::rotation::UnixMillis::new(10),
            );
            StoreUpdate::from_changed((), changed)
        })
        .unwrap();

    assert!(engines.store.load().unwrap().can_drain(
        &account,
        &thread,
        crate::rotation::UnixMillis::new(20)
    ));
}

fn credential(account: &AccountId) -> RouteCredential {
    RouteCredential {
        account_id: account.clone(),
        access_token: "token".into(),
        chatgpt_account_id: "chatgpt-a".into(),
    }
}
