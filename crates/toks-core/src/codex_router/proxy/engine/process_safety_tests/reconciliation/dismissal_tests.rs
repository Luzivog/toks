use std::{collections::BTreeSet, sync::Arc};

use crate::codex_router::proxy::catalogue::Catalogue;
use crate::codex_router::proxy::engine::{Engine, EngineConfig};
use crate::codex_router::proxy::types::SharedCredentials;
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{ThreadId, ThreadOverrideChange, UnixMillis, WorkerConnectionOwner};
use crate::storage::StoreUpdate;

use super::super::Credentials;
use super::Reconciliation;

impl Reconciliation {
    fn restart(&self) -> Arc<Engine> {
        let source: SharedCredentials = Arc::new(Credentials {
            accounts: vec![self.original_account.clone()],
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

    fn cancel_with_override(&self, thread: &ThreadId) {
        self.settings
            .update(|settings| {
                let cancelled = settings.cancel_thread(thread);
                let overridden = settings
                    .set_thread_override(
                        thread,
                        ThreadOverrideChange::ServiceTier(Some("priority".into())),
                    )
                    .unwrap();
                StoreUpdate::from_changed((), cancelled | overridden)
            })
            .unwrap();
    }
}

#[test]
fn settings_application_dismisses_a_dormant_follow_up_and_prunes_its_settings() {
    let test = Reconciliation::new();
    let dismissed = ThreadId::new("dismissed-follow-up");
    let survivor = ThreadId::new("unrelated-live-thread");
    assert!(test
        .original
        .route(&test.original_account, &dismissed)
        .unwrap()
        .is_some());
    test.original
        .continue_response(&test.original_account, &dismissed)
        .unwrap();
    assert!(test
        .original
        .route(&test.original_account, &survivor)
        .unwrap()
        .is_some());
    test.cancel_with_override(&dismissed);

    let _restarted = test.restart();

    let runtime = test.store.load().unwrap();
    assert_eq!(runtime.in_flight_count(&test.original_account), 1);
    assert_eq!(runtime.retained_thread_ids(), BTreeSet::from([survivor]));
    let settings = test.settings.load().unwrap();
    assert!(!settings.cancelled_threads().contains(&dismissed));
    assert!(settings.thread_override(&dismissed).is_none());
}

#[test]
fn settings_application_defers_a_live_thread_then_prunes_after_it_is_gone() {
    let test = Reconciliation::new();
    let thread = ThreadId::new("live-when-dismissed");
    assert!(test
        .original
        .route(&test.original_account, &thread)
        .unwrap()
        .is_some());
    test.cancel_with_override(&thread);

    test.original
        .apply_rotation_settings(UnixMillis::new(10))
        .unwrap();

    assert_eq!(
        test.store
            .load()
            .unwrap()
            .in_flight_count(&test.original_account),
        1
    );
    let settings = test.settings.load().unwrap();
    assert!(settings.cancelled_threads().contains(&thread));
    assert!(settings.thread_override(&thread).is_some());

    test.original
        .close(&test.original_account, &thread)
        .unwrap();
    test.original
        .apply_rotation_settings(UnixMillis::new(11))
        .unwrap();

    let settings = test.settings.load().unwrap();
    assert!(!settings.cancelled_threads().contains(&thread));
    assert!(settings.thread_override(&thread).is_none());
}
