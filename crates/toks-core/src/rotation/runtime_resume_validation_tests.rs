use std::fs;

use serde_json::{json, Value};

use crate::accounts::AccountId;

use super::{
    ResumeAuthorization, RotationRuntime, RotationRuntimeStore, RotationSettings, ThreadId,
    UnixMillis,
};

const ATTEMPT: &str = "00000000-0000-4000-8000-000000000001";
const OTHER_ATTEMPT: &str = "00000000-0000-4000-8000-000000000002";
const WAITING_ID: &str = "00000000-0000-4000-8000-000000000011";
const OTHER_WAITING_ID: &str = "00000000-0000-4000-8000-000000000012";

fn active_runtime() -> Value {
    let account = AccountId::new("temporarily-undiscovered");
    let thread = ThreadId::new("thread");
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();
    runtime.waiting(&thread, UnixMillis::new(1));
    let waiting = runtime.waiting_threads()[0].clone();
    assert_eq!(
        runtime.authorize_resume(
            &settings,
            std::slice::from_ref(&account),
            &waiting,
            ATTEMPT,
            &account,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Acquired
    );
    serde_json::to_value(runtime).unwrap()
}

fn admission(value: &mut Value) -> &mut Value {
    value["resumeAdmissions"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()
}

fn load(value: &Value) -> anyhow::Result<RotationRuntime> {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(store.path(), serde_json::to_vec(value).unwrap()).unwrap();
    store.load()
}

#[test]
fn load_rejects_resume_admission_key_identity_mismatch() {
    let mut value = active_runtime();
    let admissions = value["resumeAdmissions"].as_object_mut().unwrap();
    let key = admissions.keys().next().unwrap().clone();
    let admission = admissions.remove(&key).unwrap();
    admissions.insert(WAITING_ID.into(), admission);

    assert!(load(&value)
        .unwrap_err()
        .to_string()
        .contains("key does not match"));
}

#[test]
fn load_rejects_noncanonical_attempt_and_unrecognized_waiting_identity() {
    let mut attempt = active_runtime();
    admission(&mut attempt)["attempt"] = json!("00000000-0000-4000-8000-0000000000AA");
    assert!(load(&attempt)
        .unwrap_err()
        .to_string()
        .contains("canonical UUID"));

    let mut waiting = active_runtime();
    waiting["waitingThreads"] = json!([{
        "waitingId": "not-an-id",
        "threadId": "other",
        "since": 3
    }]);
    assert!(load(&waiting)
        .unwrap_err()
        .to_string()
        .contains("unrecognized waiting identity"));
}

#[test]
fn load_rejects_duplicate_active_thread_and_attempt_bindings() {
    let mut duplicate_thread = active_runtime();
    let mut second = admission(&mut duplicate_thread).clone();
    second["attempt"] = json!(OTHER_ATTEMPT);
    second["waiting"]["waitingId"] = json!(OTHER_WAITING_ID);
    duplicate_thread["resumeAdmissions"]
        .as_object_mut()
        .unwrap()
        .insert(OTHER_WAITING_ID.into(), second);
    assert!(load(&duplicate_thread)
        .unwrap_err()
        .to_string()
        .contains("duplicate active resume thread"));

    let mut duplicate_attempt = active_runtime();
    let mut second = admission(&mut duplicate_attempt).clone();
    second["waiting"] = json!({
        "waitingId": OTHER_WAITING_ID,
        "threadId": "other",
        "since": 3
    });
    duplicate_attempt["resumeAdmissions"]
        .as_object_mut()
        .unwrap()
        .insert(OTHER_WAITING_ID.into(), second);
    assert!(load(&duplicate_attempt)
        .unwrap_err()
        .to_string()
        .contains("duplicate resume attempt"));
}

#[test]
fn load_rejects_active_waiting_overlap_and_missing_finished_replacement() {
    let mut active_overlap = active_runtime();
    let original = admission(&mut active_overlap)["waiting"].clone();
    active_overlap["waitingThreads"] = json!([original]);
    assert!(load(&active_overlap)
        .unwrap_err()
        .to_string()
        .contains("also waiting"));

    let mut finished = active_runtime();
    admission(&mut finished)["phase"] = json!({
        "finished": {"waiting_id": WAITING_ID}
    });
    assert!(load(&finished)
        .unwrap_err()
        .to_string()
        .contains("replacement is missing"));
}

#[test]
fn valid_terminal_tombstone_and_temporarily_undiscovered_account_load() {
    let mut value = active_runtime();
    value["accounts"] = json!({});
    let mut replacement = admission(&mut value)["waiting"].clone();
    replacement["waitingId"] = json!(WAITING_ID);
    replacement["since"] = json!(3);
    value["waitingThreads"] = json!([replacement]);
    admission(&mut value)["phase"] = json!({
        "finished": {"waiting_id": WAITING_ID}
    });

    assert!(load(&value).is_ok());
}

#[test]
fn runtime_refuses_to_bind_one_attempt_capability_to_two_waiting_entries() {
    let account = AccountId::new("account");
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();
    runtime.waiting(&ThreadId::new("first"), UnixMillis::new(1));
    runtime.waiting(&ThreadId::new("second"), UnixMillis::new(1));
    let first = runtime.waiting_threads()[0].clone();
    let second = runtime.waiting_threads()[1].clone();

    assert_eq!(
        runtime.authorize_resume(
            &settings,
            std::slice::from_ref(&account),
            &first,
            ATTEMPT,
            &account,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Acquired
    );
    assert_eq!(
        runtime.authorize_resume(
            &settings,
            std::slice::from_ref(&account),
            &second,
            ATTEMPT,
            &account,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Lost
    );
    assert_eq!(runtime.waiting_threads(), &[second]);
}

#[test]
fn discarding_a_stale_waiting_identity_preserves_its_newer_replacement() {
    let mut runtime = RotationRuntime::default();
    let thread = ThreadId::new("thread");
    runtime.waiting(&thread, UnixMillis::new(1));
    let stale = runtime.waiting_threads()[0].clone();
    let replacement_id = super::WaitingId::for_test("replacement");
    let replacement = runtime
        .waiting_after_attempt(&stale, replacement_id, UnixMillis::new(2))
        .unwrap();

    assert!(!runtime.discard_waiting_entries(std::slice::from_ref(&stale)));
    assert_eq!(
        runtime.waiting_threads(),
        std::slice::from_ref(&replacement)
    );
    assert!(runtime.discard_waiting_entries(std::slice::from_ref(&replacement)));
    assert!(runtime.waiting_threads().is_empty());
}

#[test]
fn resume_authorization_preserves_current_awaiting_account_affinity() {
    let account_a = AccountId::new("account-a");
    let account_b = AccountId::new("account-b");
    let discovered = [account_a.clone(), account_b.clone()];
    let thread = ThreadId::new("awaiting-thread");
    let mut settings = RotationSettings::default();
    settings.reconcile(&discovered);
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened(&account_b, &thread, UnixMillis::new(1))
        .unwrap();
    assert!(runtime.connection_continues(&account_b, &thread, UnixMillis::new(2)));
    assert!(runtime.waiting(&thread, UnixMillis::new(3)));
    let waiting = runtime.waiting_threads()[0].clone();

    assert_eq!(
        runtime.authorize_resume(
            &settings,
            &discovered,
            &waiting,
            ATTEMPT,
            &account_a,
            UnixMillis::new(4),
        ),
        ResumeAuthorization::Stale
    );
    assert_eq!(runtime.waiting_threads(), std::slice::from_ref(&waiting));
    assert_eq!(
        runtime.authorize_resume(
            &settings,
            &discovered,
            &waiting,
            ATTEMPT,
            &account_b,
            UnixMillis::new(4),
        ),
        ResumeAuthorization::Acquired
    );
}

#[test]
fn resume_authorization_preserves_current_attached_account_affinity() {
    let account_a = AccountId::new("account-a");
    let account_b = AccountId::new("account-b");
    let discovered = [account_a.clone(), account_b.clone()];
    let thread = ThreadId::new("attached-thread");
    let mut settings = RotationSettings::default();
    settings.reconcile(&discovered);
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();
    runtime.thread_attached(&account_b, &thread).unwrap();
    assert!(runtime.waiting(&thread, UnixMillis::new(1)));
    let waiting = runtime.waiting_threads()[0].clone();

    assert_eq!(
        runtime.authorize_resume(
            &settings,
            &discovered,
            &waiting,
            ATTEMPT,
            &account_a,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Stale
    );
    assert_eq!(
        runtime.authorize_resume(
            &settings,
            &discovered,
            &waiting,
            ATTEMPT,
            &account_b,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Acquired
    );
}

#[test]
fn load_rejects_active_resume_admission_that_conflicts_with_live_ownership() {
    let mut value = active_runtime();
    value["activeThreads"] = json!({
        "thread": {
            "accountId": "different-account",
            "streams": 0,
            "reservations": 0,
            "awaitingFollowUp": true,
            "lastActivityAt": 3
        }
    });

    assert!(load(&value)
        .unwrap_err()
        .to_string()
        .contains("conflicts with live thread ownership"));
}

#[test]
fn current_cancellation_prevents_durable_resume_authorization() {
    let account = AccountId::new("account");
    let thread = ThreadId::new("cancelled-before-authorization");
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();
    runtime.waiting(&thread, UnixMillis::new(1));
    let waiting = runtime.waiting_threads()[0].clone();
    settings.cancel_thread(&thread);

    assert_eq!(
        runtime.authorize_resume(
            &settings,
            std::slice::from_ref(&account),
            &waiting,
            ATTEMPT,
            &account,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Cancelled
    );
    assert_eq!(runtime.waiting_threads(), std::slice::from_ref(&waiting));
}
