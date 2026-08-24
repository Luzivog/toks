use std::fs;

use super::account;
use crate::rotation::{RotationRuntimeStore, RotationSettingsStore, ThreadId, UnixMillis};

const CURRENT_RUNTIME_WITH_ACCOUNT_AUTH: &[u8] = br#"{
  "version": 1,
  "health": "healthy",
  "heartbeatAt": 10,
  "accounts": {
    "account": {
      "blockedUntil": 300,
      "blockConfirmed": true,
      "blockResetKnown": true,
      "quotaAuthorityRevision": 7,
      "quotaExhaustion": {
        "until": 200,
        "resetKnown": true
      },
      "grandfatheredThreads": [],
      "provisionalThreads": [],
      "threadUsage": {},
      "needsSignIn": true,
      "authFailureRevision": 3,
      "authFailedAt": 25,
      "rejectedCredentialFingerprint": "fingerprint-current",
      "rejectedCredentialHistory": [
        "fingerprint-old",
        "fingerprint-current"
      ]
    }
  },
  "activeThreads": {},
  "attachedThreads": {},
  "resumeAdmissions": {},
  "waitingThreads": [],
  "events": []
}"#;

#[test]
fn stores_reject_unknown_document_versions() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationSettingsStore::for_data_dir(directory.path());
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(
        store.path(),
        br#"{"version":99,"enabled":false,"priority":[],"excluded":[],"preferred":null,"cancelledThreads":[],"waitingPriority":[]}"#,
    )
    .unwrap();

    assert!(store.load().unwrap_err().to_string().contains("version 99"));
}

#[test]
fn runtime_written_before_thread_overrides_keeps_its_drain_affinity() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(
        store.path(),
        br#"{"version":1,"health":"healthy","heartbeatAt":1,"accounts":{"a":{"blockedUntil":null,"blockConfirmed":false,"blockResetKnown":false,"quotaExhaustion":{"until":100,"resetKnown":true},"grandfatheredThreads":["thread"],"needsSignIn":false}},"activeThreads":{},"waitingThreads":[],"events":[]}"#,
    )
    .unwrap();

    let runtime = store.load().unwrap();
    let account = account("a");
    let thread = ThreadId::new("thread");
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(50)));
    assert!(!runtime.requires_standard_tier(&account, &thread, UnixMillis::new(50)));
}

#[test]
fn account_auth_state_keeps_current_runtime_document_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(store.path(), CURRENT_RUNTIME_WITH_ACCOUNT_AUTH).unwrap();

    let runtime = store.load().unwrap();
    store.save(&runtime).unwrap();

    assert_eq!(
        fs::read(store.path()).unwrap(),
        CURRENT_RUNTIME_WITH_ACCOUNT_AUTH
    );
}

#[test]
fn legacy_fast_drain_opt_out_is_removed_when_settings_are_saved() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationSettingsStore::for_data_dir(directory.path());
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(
        store.path(),
        br#"{"version":1,"enabled":true,"priority":[],"excluded":[],"cancelledThreads":[],"waitingPriority":[],"fastWhenDraining":false}"#,
    )
    .unwrap();

    let settings = store.load().unwrap();
    store.save(&settings).unwrap();

    assert!(!fs::read_to_string(store.path())
        .unwrap()
        .contains("fastWhenDraining"));
}
