use tempfile::tempdir;

use super::retry::{clear_retry_intent, load_retry_intent, request_retry};
use crate::codex_router::host::BuildId;

#[test]
fn retry_intent_is_durable_and_one_shot() {
    let directory = tempdir().unwrap();
    let state = directory.path().join("router-host.json");
    let build = BuildId::new("candidate").unwrap();

    let intent = request_retry(&state, &build).unwrap();
    assert_eq!(load_retry_intent(&state).unwrap(), Some(intent.clone()));
    assert_eq!(request_retry(&state, &build).unwrap(), intent);
    assert!(clear_retry_intent(&state, &intent).unwrap());
    assert_eq!(load_retry_intent(&state).unwrap(), None);
}

#[test]
fn stale_build_intent_cannot_clear_the_newer_pending_retry() {
    let directory = tempdir().unwrap();
    let state = directory.path().join("router-host.json");
    let build_a = BuildId::new("build-a").unwrap();
    let build_b = BuildId::new("build-b").unwrap();

    let stale_b = request_retry(&state, &build_b).unwrap();
    let current_a = request_retry(&state, &build_a).unwrap();

    assert!(!clear_retry_intent(&state, &stale_b).unwrap());
    assert_eq!(load_retry_intent(&state).unwrap(), Some(current_a));
}

#[test]
fn stale_coordinator_cannot_clear_a_newer_nonce_for_the_same_build() {
    let directory = tempdir().unwrap();
    let state = directory.path().join("router-host.json");
    let build = BuildId::new("candidate").unwrap();
    let current = request_retry(&state, &build).unwrap();
    let stale = super::retry::RetryIntent {
        version: super::retry::RETRY_VERSION,
        build,
        id: crate::codex_router::host::RetryId::for_test(999),
    };

    assert!(!clear_retry_intent(&state, &stale).unwrap());
    assert_eq!(load_retry_intent(&state).unwrap(), Some(current));
}

#[test]
fn version_one_intent_without_a_nonce_migrates_to_a_stable_identity() {
    let directory = tempdir().unwrap();
    let state = directory.path().join("router-host.json");
    std::fs::write(
        state.with_file_name("router-host-retry.json"),
        br#"{"version":1,"build":"candidate"}"#,
    )
    .unwrap();

    let first = load_retry_intent(&state).unwrap().unwrap();
    let second = load_retry_intent(&state).unwrap().unwrap();
    assert_eq!(first, second);
    assert_eq!(first.build, BuildId::new("candidate").unwrap());
    assert_eq!(first.id.as_str(), "legacy-v1");
}

#[test]
fn retry_intent_rejects_noncanonical_persisted_ids() {
    let directory = tempdir().unwrap();
    let state = directory.path().join("router-host.json");
    for id in [
        "attacker-controlled",
        "00000000-0000-4000-8000-00000000000A",
    ] {
        std::fs::write(
            state.with_file_name("router-host-retry.json"),
            format!(r#"{{"version":1,"build":"candidate","id":"{id}"}}"#),
        )
        .unwrap();
        assert!(load_retry_intent(&state).is_err());
    }
}
