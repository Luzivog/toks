use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use crate::accounts::{
    AccountId, AccountIdentityKind, AccountProfile, CredentialProfileId, ProviderAccount,
    ProviderLimitCollection,
};
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::limits::{LimitIssue, LimitIssueKind, SnapshotFreshness, SnapshotStatus};
use crate::rotation::{
    AccountAvailability, RotationRuntimeStore, RotationSettings, RotationSettingsStore, ThreadId,
    UnixMillis,
};

use super::{credential_with_token, snapshot, CredentialState, RepairableCredentials};
use crate::codex_router::proxy::catalogue::Catalogue;
use crate::codex_router::proxy::engine::{Engine, EngineConfig};
use crate::codex_router::proxy::types::SharedCredentials;

#[tokio::test]
async fn permanent_unauthorized_state_survives_restart_until_credentials_are_proven_repaired() {
    let directory = tempfile::tempdir().unwrap();
    let account = AccountId::new("repaired");
    let credentials = Arc::new(RepairableCredentials {
        account: account.clone(),
        discovered: Mutex::new(true),
        state: Mutex::new(CredentialState::Valid("rejected-token")),
        proof_gate: Mutex::new(None),
    });
    let settings = RotationSettingsStore::for_data_dir(directory.path());
    let mut configured = RotationSettings::default();
    configured.reconcile(std::slice::from_ref(&account));
    configured.set_enabled(true);
    settings.save(&configured).unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let build = || {
        let source: SharedCredentials = credentials.clone();
        Engine::new(EngineConfig {
            credentials: source,
            settings: settings.clone(),
            runtime_store: store.clone(),
            catalogue: Catalogue::at(None),
            connection_owner: None,
            thread_sources: ThreadSourceStore::discover(),
        })
        .unwrap()
    };
    let engine = build();
    engine
        .permanent_auth_failure(&credential_with_token(&account, "rejected-token"))
        .unwrap();
    assert!(!std::fs::read_to_string(store.path())
        .unwrap()
        .contains("rejected-token"));
    drop(engine);

    *credentials.discovered.lock().unwrap() = false;
    *credentials.state.lock().unwrap() = CredentialState::NeedsSignIn;
    let engine = build();
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("bad-auth")), &Default::default())
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        store.load().unwrap().accounts()[&account].availability(UnixMillis::new(i64::MAX)),
        AccountAvailability::NeedsSignIn
    );
    drop(engine);
    assert!(store.load().unwrap().accounts().contains_key(&account));

    *credentials.discovered.lock().unwrap() = true;
    let engine = build();

    for state in [CredentialState::Unreadable, CredentialState::WrongAccount] {
        *credentials.state.lock().unwrap() = state;
        assert!(engine
            .select_for_thread(Some(&ThreadId::new("invalid-auth")), &Default::default())
            .await
            .unwrap()
            .is_none());
        assert!(store.load().unwrap().accounts()[&account].needs_sign_in());
    }
    *credentials.state.lock().unwrap() = CredentialState::Valid("rejected-token");
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("same-auth")), &Default::default())
        .await
        .unwrap()
        .is_none());
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    *credentials.state.lock().unwrap() = CredentialState::Valid("repaired-token");
    let repaired = build();
    let selected = repaired
        .select_for_thread(Some(&ThreadId::new("repaired-auth")), &Default::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.account_id, account);
    assert!(!store.load().unwrap().accounts()[&account].needs_sign_in());
}

#[tokio::test]
async fn snapshot_repair_requires_a_changed_exact_credential_and_unchanged_failure_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let auth_directory = directory.path().join("auth");
    std::fs::create_dir_all(&auth_directory).unwrap();
    write_codex_auth(&auth_directory, "rejected-token");
    let profile = codex_profile(&auth_directory);
    let rejected_auth = crate::accounts::read_codex_auth_for_test(&profile).unwrap();
    let account = rejected_auth.account_id.clone();
    let credentials = Arc::new(RepairableCredentials {
        account: account.clone(),
        discovered: Mutex::new(true),
        state: Mutex::new(CredentialState::NeedsSignIn),
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
    })
    .unwrap();
    engine
        .permanent_auth_failure(&credential_with_token(&account, "rejected-token"))
        .unwrap();

    let mut cached = snapshot(&account);
    cached.status.freshness = SnapshotFreshness::Cached;
    engine
        .apply_unproven_snapshots(&[cached], chrono::Utc::now())
        .unwrap();
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    let proof_at = chrono::Utc::now() + chrono::Duration::seconds(1);
    let mut rejected = snapshot(&account);
    rejected.status = SnapshotStatus {
        freshness: SnapshotFreshness::Live,
        last_attempted_at: Some(proof_at),
        issue: Some(LimitIssue::new(
            LimitIssueKind::Authentication,
            "synthetic rejected credential",
        )),
    };
    engine
        .apply_unproven_snapshots(&[rejected], chrono::Utc::now())
        .unwrap();
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    let mut live = snapshot(&account);
    live.status = SnapshotStatus {
        freshness: SnapshotFreshness::Live,
        last_attempted_at: Some(proof_at),
        ..SnapshotStatus::default()
    };
    let first_epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![live.clone()],
                codex_auth: vec![rejected_auth.proof()],
            },
            &first_epoch,
            chrono::Utc::now(),
        )
        .unwrap();
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    write_codex_auth(&auth_directory, "repaired-token");
    let repaired_auth = crate::accounts::read_codex_auth_for_test(&profile).unwrap();
    assert_eq!(repaired_auth.account_id, account);
    let stale_epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .permanent_auth_failure(&credential_with_token(&account, "repaired-token"))
        .unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![live.clone()],
                codex_auth: vec![repaired_auth.proof()],
            },
            &stale_epoch,
            chrono::Utc::now(),
        )
        .unwrap();
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    let matching_epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![live.clone()],
                codex_auth: vec![repaired_auth.proof()],
            },
            &matching_epoch,
            chrono::Utc::now(),
        )
        .unwrap();
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    write_codex_auth(&auth_directory, "third-token");
    let changed_auth = crate::accounts::read_codex_auth_for_test(&profile).unwrap();
    let changed_epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![live],
                codex_auth: vec![changed_auth.proof()],
            },
            &changed_epoch,
            chrono::Utc::now(),
        )
        .unwrap();
    assert!(!store.load().unwrap().accounts()[&account].needs_sign_in());

    *credentials.state.lock().unwrap() = CredentialState::Valid("rejected-token");
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("a-b-a-rollback")), &Default::default())
        .await
        .unwrap()
        .is_none());
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());

    *credentials.state.lock().unwrap() = CredentialState::Valid("fourth-token");
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("new-repair")), &Default::default())
        .await
        .unwrap()
        .is_some());
    *credentials.state.lock().unwrap() = CredentialState::Valid("rejected-token");
    assert!(engine
        .refresh(&credential_with_token(&account, "fourth-token"))
        .await
        .unwrap()
        .is_none());
    assert!(store.load().unwrap().accounts()[&account].needs_sign_in());
}

fn codex_profile(directory: &std::path::Path) -> AccountProfile {
    let profile_id = CredentialProfileId::new("snapshot-proof");
    AccountProfile {
        provider: crate::limits::Provider::Codex,
        profile_id: profile_id.clone(),
        account: ProviderAccount {
            id: AccountId::new(format!("codex-profile-{profile_id}")),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: None,
            sources: Vec::new(),
        },
        home_dir: directory.into(),
        config_dir: directory.into(),
        managed: false,
        created_at_ms: None,
    }
}

fn write_codex_auth(directory: &std::path::Path, access_token: &str) {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "iss": "https://auth.openai.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "chatgpt-repaired"
            }
        })
        .to_string(),
    );
    let signature = URL_SAFE_NO_PAD.encode([7_u8; 256]);
    let next = directory.join("auth.next");
    std::fs::write(
        &next,
        serde_json::json!({"tokens": {
            "id_token": format!("{header}.{claims}.{signature}"),
            "access_token": access_token,
            "refresh_token": "refresh",
            "account_id": "chatgpt-repaired"
        }})
        .to_string(),
    )
    .unwrap();
    std::fs::rename(next, directory.join("auth.json")).unwrap();
}

#[tokio::test]
async fn a_cross_account_credential_is_rejected_without_leaking_its_reservation() {
    let directory = tempfile::tempdir().unwrap();
    let account = AccountId::new("requested");
    let credentials: SharedCredentials = Arc::new(RepairableCredentials {
        account: account.clone(),
        discovered: Mutex::new(true),
        state: Mutex::new(CredentialState::WrongAccount),
        proof_gate: Mutex::new(None),
    });
    let settings = RotationSettingsStore::for_data_dir(directory.path());
    let mut configured = RotationSettings::default();
    configured.reconcile(std::slice::from_ref(&account));
    configured.set_enabled(true);
    settings.save(&configured).unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let engine = Engine::new(EngineConfig {
        credentials,
        settings,
        runtime_store: store.clone(),
        catalogue: Catalogue::at(None),
        connection_owner: None,
        thread_sources: ThreadSourceStore::discover(),
    })
    .unwrap();

    let error = engine
        .select_for_thread(Some(&ThreadId::new("wrong-identity")), &Default::default())
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("credential source returned account"));
    assert_eq!(store.load().unwrap().in_flight_count(&account), 0);
}
