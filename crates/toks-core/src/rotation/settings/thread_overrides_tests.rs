use std::fs;

use serde_json::{json, Value};

use super::{RotationSettings, ThreadOverrideChange};
use crate::accounts::AccountId;
use crate::rotation::{
    ResumeAuthorization, ResumeTerminal, RotationRuntime, RotationSettingsStore, ThreadId,
    UnixMillis, WaitingId,
};

const DAY_MILLIS: i64 = 24 * 60 * 60 * 1_000;
const NOW: UnixMillis = UnixMillis::new(200_000_000);
const ACTIVE_ATTEMPT: &str = "00000000-0000-4000-8000-000000000101";
const FINISHED_ATTEMPT: &str = "00000000-0000-4000-8000-000000000102";

#[test]
fn thread_overrides_round_trip_with_camel_case_fields() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationSettingsStore::for_data_dir(directory.path());
    let thread = ThreadId::new("thread");
    let mut settings = RotationSettings::default();
    assert!(settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::Model(Some("gpt-5.6-sol".into())),
        )
        .unwrap());
    assert!(settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ReasoningEffort(Some("xhigh".into())),
        )
        .unwrap());
    assert!(settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ServiceTier(Some("priority".into())),
        )
        .unwrap());

    store.save(&settings).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded, settings);
    let saved: Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(saved["threadOverrides"]["thread"]["model"], "gpt-5.6-sol");
    assert_eq!(
        saved["threadOverrides"]["thread"]["reasoningEffort"],
        "xhigh"
    );
    assert_eq!(
        saved["threadOverrides"]["thread"]["serviceTier"],
        "priority"
    );
    let thread_override = loaded.thread_override(&thread).unwrap();
    assert_eq!(thread_override.model(), Some("gpt-5.6-sol"));
    assert_eq!(thread_override.reasoning_effort(), Some("xhigh"));
    assert_eq!(thread_override.service_tier(), Some("priority"));
}

#[test]
fn settings_written_before_thread_overrides_load_with_an_empty_map() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationSettingsStore::for_data_dir(directory.path());
    let mut document = serde_json::to_value(RotationSettings::default()).unwrap();
    document.as_object_mut().unwrap().remove("threadOverrides");
    write_document(&store, &document);

    assert!(store.load().unwrap().thread_overrides().is_empty());
}

#[test]
fn load_drops_empty_entries_and_only_invalid_individual_values() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationSettingsStore::for_data_dir(directory.path());
    let mut document = serde_json::to_value(RotationSettings::default()).unwrap();
    document["threadOverrides"] = json!({
        "empty": {},
        "partially-valid": {
            "model": " untrimmed",
            "reasoningEffort": "future-effort",
            "serviceTier": "bad\u{7}"
        }
    });
    write_document(&store, &document);

    let settings = store.load().unwrap();
    assert_eq!(settings.thread_overrides().len(), 1);
    let thread_override = settings
        .thread_override(&ThreadId::new("partially-valid"))
        .unwrap();
    assert_eq!(thread_override.model(), None);
    assert_eq!(thread_override.reasoning_effort(), Some("future-effort"));
    assert_eq!(thread_override.service_tier(), None);
}

#[test]
fn override_changes_validate_shape_without_restricting_catalogue_values() {
    let thread = ThreadId::new("thread");
    let mut settings = RotationSettings::default();
    assert!(settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::Model(Some("future-model".into())),
        )
        .unwrap());
    assert!(!settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::Model(Some("future-model".into())),
        )
        .unwrap());

    for invalid in ["", " leading", "trailing ", "line\nbreak", "control\u{7}"] {
        let before = settings.clone();
        assert!(settings
            .set_thread_override(
                &thread,
                ThreadOverrideChange::ServiceTier(Some(invalid.into())),
            )
            .is_err());
        assert_eq!(settings, before);
    }

    assert!(settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ServiceTier(Some("future-tier".into())),
        )
        .unwrap());
    assert!(settings
        .set_thread_override(&thread, ThreadOverrideChange::Model(None))
        .unwrap());
    let thread_override = settings.thread_override(&thread).unwrap();
    assert_eq!(thread_override.model(), None);
    assert_eq!(thread_override.service_tier(), Some("future-tier"));
    assert!(settings
        .set_thread_override(&thread, ThreadOverrideChange::ServiceTier(None))
        .unwrap());
    assert!(settings.thread_override(&thread).is_none());
    assert!(!settings
        .set_thread_override(&thread, ThreadOverrideChange::Model(None))
        .unwrap());
}

#[test]
fn override_pruning_keeps_present_and_recent_threads() {
    let account = AccountId::new("account");
    let active = ThreadId::new("active");
    let attached = ThreadId::new("attached");
    let waiting = ThreadId::new("waiting");
    let active_resume = ThreadId::new("active-resume");
    let finished_resume = ThreadId::new("finished-resume");
    let recent = ThreadId::new("recent");
    let boundary = ThreadId::new("boundary");
    let stale = ThreadId::new("stale");
    let absent = ThreadId::new("absent");
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();
    runtime
        .reserve_thread(&account, &active, UnixMillis::new(1))
        .unwrap();
    runtime.thread_attached(&account, &attached).unwrap();
    assert!(runtime.waiting(&waiting, UnixMillis::new(1)));
    authorize_resume(
        &mut runtime,
        &settings,
        &account,
        &active_resume,
        ACTIVE_ATTEMPT,
        false,
    );
    authorize_resume(
        &mut runtime,
        &settings,
        &account,
        &finished_resume,
        FINISHED_ATTEMPT,
        true,
    );
    route_then_finish(
        &mut runtime,
        &account,
        &recent,
        UnixMillis::new(NOW.get() - DAY_MILLIS + 1),
    );
    route_then_finish(
        &mut runtime,
        &account,
        &boundary,
        UnixMillis::new(NOW.get() - DAY_MILLIS),
    );
    route_then_finish(
        &mut runtime,
        &account,
        &stale,
        UnixMillis::new(NOW.get() - DAY_MILLIS - 1),
    );
    for thread in [
        &active,
        &attached,
        &waiting,
        &active_resume,
        &finished_resume,
        &recent,
        &boundary,
        &stale,
        &absent,
    ] {
        settings
            .set_thread_override(thread, ThreadOverrideChange::Model(Some("model".into())))
            .unwrap();
    }

    assert!(settings.reconcile_thread_overrides(&runtime, NOW));
    assert_eq!(
        settings
            .thread_overrides()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            active,
            active_resume,
            attached,
            finished_resume,
            recent,
            waiting,
        ]
    );
    assert!(!settings.reconcile_thread_overrides(&runtime, NOW));
}

fn write_document(store: &RotationSettingsStore, document: &Value) {
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(store.path(), serde_json::to_vec(document).unwrap()).unwrap();
}

fn authorize_resume(
    runtime: &mut RotationRuntime,
    settings: &RotationSettings,
    account: &AccountId,
    thread: &ThreadId,
    attempt: &str,
    finish: bool,
) {
    assert!(runtime.waiting(thread, UnixMillis::new(1)));
    let waiting = runtime
        .waiting_threads()
        .iter()
        .find(|waiting| &waiting.thread_id == thread)
        .unwrap()
        .clone();
    assert_eq!(
        runtime.authorize_resume(
            settings,
            std::slice::from_ref(account),
            &waiting,
            attempt,
            account,
            UnixMillis::new(2),
        ),
        ResumeAuthorization::Acquired
    );
    if finish {
        runtime.finish_resume(
            &waiting,
            attempt,
            ResumeTerminal::Success,
            WaitingId::for_attempt(attempt),
            UnixMillis::new(3),
        );
    }
}

fn route_then_finish(
    runtime: &mut RotationRuntime,
    account: &AccountId,
    thread: &ThreadId,
    at: UnixMillis,
) {
    runtime.connection_opened(account, thread, at).unwrap();
    assert!(runtime.connection_closed(account, thread, at));
}
