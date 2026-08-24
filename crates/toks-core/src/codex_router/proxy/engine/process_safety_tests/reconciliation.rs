use std::sync::Arc;

use crate::accounts::AccountId;
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{
    RotationRuntimeStore, RotationSettings, RotationSettingsStore, ThreadId, WorkerConnectionOwner,
};

use super::super::super::catalogue::Catalogue;
use super::super::super::types::SharedCredentials;
use super::super::{Engine, EngineConfig};
use super::Credentials;

struct Reconciliation {
    _directory: tempfile::TempDir,
    settings: RotationSettingsStore,
    store: RotationRuntimeStore,
    original: Arc<Engine>,
    original_account: AccountId,
    challenger_account: AccountId,
}

impl Reconciliation {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let original_account = AccountId::new("original");
        let challenger_account = AccountId::new("challenger");
        let settings_store = RotationSettingsStore::for_data_dir(directory.path());
        let mut settings = RotationSettings::default();
        settings.reconcile(&[original_account.clone(), challenger_account.clone()]);
        settings.set_enabled(true);
        settings_store.save(&settings).unwrap();
        let store = RotationRuntimeStore::for_data_dir(directory.path());
        let source: SharedCredentials = Arc::new(Credentials {
            accounts: vec![original_account.clone()],
        });
        let original = Engine::new(EngineConfig {
            credentials: source,
            settings: settings_store.clone(),
            runtime_store: store.clone(),
            catalogue: Catalogue::at(None),
            connection_owner: Some(WorkerConnectionOwner::new(1, 101).unwrap()),
            thread_sources: ThreadSourceStore::discover(),
        })
        .unwrap();
        assert!(store
            .load()
            .unwrap()
            .accounts()
            .contains_key(&original_account));
        Self {
            _directory: directory,
            settings: settings_store,
            store,
            original,
            original_account,
            challenger_account,
        }
    }

    fn lose_discovery(&self) -> Arc<Engine> {
        let source: SharedCredentials = Arc::new(Credentials {
            accounts: vec![self.challenger_account.clone()],
        });
        Engine::new(EngineConfig {
            credentials: source,
            settings: self.settings.clone(),
            runtime_store: self.store.clone(),
            catalogue: Catalogue::at(None),
            connection_owner: Some(WorkerConnectionOwner::new(2, 201).unwrap()),
            thread_sources: ThreadSourceStore::discover(),
        })
        .unwrap()
    }

    fn assert_challenger_blocked(&self, challenger: &Engine, thread: &ThreadId) {
        assert!(challenger
            .route(&self.challenger_account, thread)
            .unwrap()
            .is_none());
        assert!(!challenger.attach(&self.challenger_account, thread).unwrap());
    }

    fn assert_challenger_can_finish(&self, challenger: &Engine, thread: &ThreadId) {
        assert!(challenger
            .route(&self.challenger_account, thread)
            .unwrap()
            .is_some());
        challenger.close(&self.challenger_account, thread).unwrap();
    }
}

#[tokio::test]
async fn account_discovery_loss_preserves_a_live_reservation_until_release() {
    let test = Reconciliation::new();
    let thread = ThreadId::new("reserved-before-discovery-loss");
    test.original
        .select_for_thread(Some(&thread), &Default::default())
        .await
        .unwrap()
        .unwrap();

    let challenger = test.lose_discovery();
    test.assert_challenger_blocked(&challenger, &thread);
    test.original
        .release_reservation(&test.original_account, &thread)
        .unwrap();
    test.assert_challenger_can_finish(&challenger, &thread);
}

#[test]
fn account_discovery_loss_preserves_a_worker_stream_until_late_close() {
    let test = Reconciliation::new();
    let thread = ThreadId::new("stream-before-discovery-loss");
    assert!(test
        .original
        .route(&test.original_account, &thread)
        .unwrap()
        .is_some());

    let challenger = test.lose_discovery();
    test.assert_challenger_blocked(&challenger, &thread);
    test.original
        .close(&test.original_account, &thread)
        .unwrap();
    test.assert_challenger_can_finish(&challenger, &thread);
}

#[test]
fn account_discovery_loss_preserves_a_websocket_attachment_until_late_detach() {
    let test = Reconciliation::new();
    let thread = ThreadId::new("attachment-before-discovery-loss");
    assert!(test
        .original
        .attach(&test.original_account, &thread)
        .unwrap());

    let challenger = test.lose_discovery();
    test.assert_challenger_blocked(&challenger, &thread);
    test.original
        .detach(&test.original_account, &thread)
        .unwrap();
    test.assert_challenger_can_finish(&challenger, &thread);
}

#[test]
fn account_discovery_loss_preserves_follow_up_affinity_until_final_close() {
    let test = Reconciliation::new();
    let thread = ThreadId::new("follow-up-before-discovery-loss");
    assert!(test
        .original
        .route(&test.original_account, &thread)
        .unwrap()
        .is_some());
    test.original
        .continue_response(&test.original_account, &thread)
        .unwrap();

    let challenger = test.lose_discovery();
    test.assert_challenger_blocked(&challenger, &thread);
    assert!(test
        .original
        .route(&test.original_account, &thread)
        .unwrap()
        .is_some());
    test.original
        .close(&test.original_account, &thread)
        .unwrap();
    test.assert_challenger_can_finish(&challenger, &thread);
    assert_eq!(
        test.store
            .load()
            .unwrap()
            .active_threads(&test.original_account),
        0
    );
}
