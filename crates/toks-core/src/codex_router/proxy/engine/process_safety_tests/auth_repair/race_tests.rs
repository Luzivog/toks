use std::sync::{Arc, Mutex};

use crate::accounts::AccountId;
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{RotationRuntimeStore, RotationSettings, RotationSettingsStore, ThreadId};

use super::{credential_with_token, CredentialState, RepairableCredentials};
use crate::codex_router::proxy::catalogue::Catalogue;
use crate::codex_router::proxy::engine::{Engine, EngineConfig};
use crate::codex_router::proxy::types::SharedCredentials;

#[tokio::test]
async fn a_newer_unauthorized_response_wins_over_an_inflight_repair_proof() {
    let directory = tempfile::tempdir().unwrap();
    let account = AccountId::new("repair-race");
    let credentials = Arc::new(RepairableCredentials {
        account: account.clone(),
        discovered: Mutex::new(true),
        state: Mutex::new(CredentialState::Valid("token-a")),
        proof_gate: Mutex::new(None),
    });
    let settings = RotationSettingsStore::for_data_dir(directory.path());
    let mut configured = RotationSettings::default();
    configured.reconcile(std::slice::from_ref(&account));
    configured.set_enabled(true);
    settings.save(&configured).unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let source: SharedCredentials = credentials.clone();
    let engine = Engine::new(EngineConfig {
        credentials: source,
        settings,
        runtime_store: store.clone(),
        catalogue: Catalogue::at(None),
        connection_owner: None,
        thread_sources: ThreadSourceStore::discover(),
        task_activity_store: None,
    })
    .unwrap();
    engine
        .permanent_auth_failure(&credential_with_token(&account, "token-a"))
        .unwrap();
    *credentials.state.lock().unwrap() = CredentialState::Valid("token-b");
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let proceed = Arc::new(tokio::sync::Notify::new());
    *credentials.proof_gate.lock().unwrap() = Some((started_tx, proceed.clone()));
    let selecting = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .select_for_thread(Some(&ThreadId::new("repair-race")), &Default::default())
                .await
                .unwrap()
        }
    });

    started_rx.recv().await.unwrap();
    engine
        .permanent_auth_failure(&credential_with_token(&account, "token-b"))
        .unwrap();
    proceed.notify_one();
    assert!(selecting.await.unwrap().is_none());
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    *credentials.proof_gate.lock().unwrap() = None;
    *credentials.state.lock().unwrap() = CredentialState::Valid("token-c");
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("repair-race")), &Default::default())
        .await
        .unwrap()
        .is_some());
    assert!(!store.load().unwrap().accounts()[&account].needs_sign_in());
}
