use std::fs;

use crate::accounts::AccountId;

use super::{
    RotationEventKind, RotationRuntime, RotationRuntimeStore, RotationSettings,
    RotationSettingsStore, RouterHealth, ThreadId, UnixMillis,
};

fn account(id: &str) -> AccountId {
    AccountId::new(id)
}

#[test]
fn settings_reconcile_and_user_mutations_preserve_a_total_priority_order() {
    let a = account("a");
    let b = account("b");
    let c = account("c");
    let mut settings = RotationSettings::default();

    assert!(settings.reconcile(&[a.clone(), b.clone(), a.clone()]));
    assert_eq!(settings.priority(), &[a.clone(), b.clone()]);
    assert!(!settings.reconcile(&[a.clone(), b.clone()]));
    assert!(settings.reconcile(&[b.clone(), c.clone()]));
    assert_eq!(settings.priority(), &[b.clone(), c.clone()]);

    assert!(settings.set_enabled(true));
    assert!(!settings.set_enabled(true));
    assert!(settings.move_to(&c, 0));
    assert_eq!(settings.priority(), &[c.clone(), b.clone()]);
    assert!(settings.use_now(&b));
    assert_eq!(settings.preferred(), Some(&b));
    assert!(settings.set_included(&b, false));
    assert_eq!(settings.preferred(), None);
    assert!(settings.excluded().contains(&b));
}

#[test]
fn selection_honors_preference_and_skips_only_currently_unavailable_accounts() {
    let a = account("a");
    let b = account("b");
    let c = account("c");
    let discovered = [a.clone(), b.clone(), c.clone()];
    let mut settings = RotationSettings::default();
    settings.reconcile(&discovered);
    settings.set_enabled(true);
    settings.use_now(&c);

    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&discovered, UnixMillis::new(0));
    runtime.block(&c, UnixMillis::new(20), UnixMillis::new(1));
    runtime.auth_failed(&a, UnixMillis::new(2));

    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(10)),
        Some(b.clone())
    );
    runtime.block(&b, UnixMillis::new(20), UnixMillis::new(3));
    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(10)),
        None
    );
    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(20)),
        Some(c)
    );
}

#[test]
fn waiting_queue_controls_are_settings_owned_and_idempotent() {
    let first = ThreadId::new("first");
    let second = ThreadId::new("second");
    let third = ThreadId::new("third");
    let mut settings = RotationSettings::default();

    assert!(settings.reconcile_waiting(&[first.clone(), second.clone(), third.clone()]));
    assert_eq!(
        settings.waiting_priority(),
        &[first.clone(), second.clone(), third.clone()]
    );
    assert!(settings.move_waiting_to(&third, 0));
    assert!(settings.cancel_waiting(&second));
    assert!(!settings.cancel_waiting(&second));
    assert!(settings.cancelled_threads().contains(&second));
    assert_eq!(settings.waiting_priority(), &[third.clone(), first.clone()]);

    assert!(settings.restore_waiting(&second));
    assert!(!settings.restore_waiting(&second));
    assert_eq!(
        settings.waiting_priority(),
        &[third.clone(), first.clone(), second.clone()]
    );
    assert!(settings.reconcile_waiting(&[second.clone(), third.clone()]));
    assert_eq!(settings.waiting_priority(), &[third, second]);
}

#[test]
fn runtime_tracks_streams_waiting_and_metadata_events_idempotently() {
    let a = account("a");
    let b = account("b");
    let thread = ThreadId::new("thread-1");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[a.clone(), b.clone()], UnixMillis::new(0));
    runtime.heartbeat(UnixMillis::new(1));
    runtime.connection_opened(&a, &thread, UnixMillis::new(2));
    runtime.rotated(&thread, &a, &b, UnixMillis::new(3));
    assert!(runtime.block(&a, UnixMillis::new(50), UnixMillis::new(4)));
    assert!(!runtime.block(&a, UnixMillis::new(50), UnixMillis::new(4)));
    assert!(runtime.auth_failed(&b, UnixMillis::new(5)));
    assert!(!runtime.auth_failed(&b, UnixMillis::new(5)));
    assert!(runtime.waiting(&thread, UnixMillis::new(6)));
    assert!(!runtime.waiting(&thread, UnixMillis::new(6)));
    assert!(runtime.resumed(&thread, &b, UnixMillis::new(7)));
    assert!(!runtime.resumed(&thread, &b, UnixMillis::new(7)));

    assert_eq!(runtime.health(), RouterHealth::Healthy);
    assert_eq!(runtime.heartbeat_at(), Some(UnixMillis::new(1)));
    assert_eq!(runtime.accounts()[&a].active_streams(), 1);
    assert!(runtime.connection_closed(&a));
    assert!(!runtime.connection_closed(&a));
    assert!(runtime.waiting_threads().is_empty());
    assert!(matches!(
        runtime.events().front().map(|event| &event.event),
        Some(RotationEventKind::Resumed { .. })
    ));

    let json = serde_json::to_string(&runtime).unwrap();
    for forbidden in ["prompt", "response", "tokens", "authorization"] {
        assert!(!json.contains(forbidden));
    }
}

#[test]
fn runtime_keeps_the_newest_hundred_events() {
    let mut runtime = RotationRuntime::default();
    for at in 0..105 {
        runtime.router_failed(UnixMillis::new(at));
    }

    assert_eq!(runtime.events().len(), 100);
    assert_eq!(runtime.events().front().unwrap().at, UnixMillis::new(104));
    assert_eq!(runtime.events().back().unwrap().at, UnixMillis::new(5));
}

#[test]
fn separate_stores_round_trip_atomically_with_private_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let settings_store = RotationSettingsStore::for_data_dir(directory.path());
    let runtime_store = RotationRuntimeStore::for_data_dir(directory.path());
    assert_ne!(settings_store.path(), runtime_store.path());
    assert_eq!(settings_store.load().unwrap(), RotationSettings::default());
    assert_eq!(runtime_store.load().unwrap(), RotationRuntime::default());

    let a = account("a");
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&a));
    settings.set_enabled(true);
    settings_store.save(&settings).unwrap();
    settings_store.save(&settings).unwrap();

    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&a), UnixMillis::new(0));
    runtime.heartbeat(UnixMillis::new(10));
    runtime_store.save(&runtime).unwrap();
    runtime_store.save(&runtime).unwrap();

    assert_eq!(settings_store.load().unwrap(), settings);
    assert_eq!(runtime_store.load().unwrap(), runtime);
    let state_dir = settings_store.path().parent().unwrap();
    assert_eq!(fs::read_dir(state_dir).unwrap().count(), 2);
    assert!(fs::read_to_string(settings_store.path())
        .unwrap()
        .contains("\"version\": 1"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(state_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for path in [settings_store.path(), runtime_store.path()] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

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
