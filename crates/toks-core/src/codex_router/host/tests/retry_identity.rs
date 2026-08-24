use super::super::model::DeployError;
use super::super::{DeploymentEvent, DeploymentState, RetryId};
use super::{build, stage};

#[test]
fn retry_ids_accept_only_canonical_uuids_and_the_legacy_sentinel() {
    assert!(RetryId::new("00000000-0000-4000-8000-000000000001").is_ok());
    assert!(RetryId::new("legacy-v1").is_ok());
    for invalid in [
        "",
        "retry-one",
        "LEGACY-V1",
        "{00000000-0000-4000-8000-000000000001}",
        "00000000-0000-4000-8000-00000000000A",
    ] {
        assert_eq!(RetryId::new(invalid), Err(DeployError::InvalidRetryId));
    }
}

#[test]
fn generated_retry_ids_are_canonical_and_distinct() {
    let first = RetryId::fresh();
    let second = RetryId::fresh();

    assert!(first.is_valid());
    assert!(second.is_valid());
    assert_ne!(first, second);
    assert_eq!(first.as_str(), first.as_str().to_ascii_lowercase());
}

#[test]
fn persisted_retry_receipts_reject_noncanonical_ids() {
    let mut state = DeploymentState::default();
    let failed = stage(&mut state, "candidate");
    state
        .reconcile(DeploymentEvent::Failed {
            generation: failed,
            reason: "failed".into(),
        })
        .unwrap();
    let retry = RetryId::for_test(8);
    state
        .consume_retry(build("candidate"), retry.clone())
        .unwrap();
    let mut stored = serde_json::to_value(state).unwrap();
    let receipts = stored["retryReceipts"].as_object_mut().unwrap();
    let receipt = receipts.remove(retry.as_str()).unwrap();
    receipts.insert("not-a-retry-id".into(), receipt);
    let recovered: DeploymentState = serde_json::from_value(stored).unwrap();

    assert_eq!(
        recovered.validate(),
        Err(DeployError::InvalidPersistedState("invalid retry receipt"))
    );
}
