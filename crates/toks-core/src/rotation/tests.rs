use std::{collections::BTreeMap, fs};

use crate::accounts::AccountId;

use super::{
    AccountAvailability, BlockWindow, QuotaObservation, RotationEventKind, RotationRuntime,
    RotationRuntimeStore, RotationSettings, RotationSettingsStore, RouterHealth, ThreadId,
    UnixMillis, UsageLimitClassification, UsageLimitEvidence, UsageLimitIncident, UsageLimitPhase,
    UsageLimitTier, UsageLimitTierOrigin, WaitingId, WaitingThread,
};
fn account(id: &str) -> AccountId {
    AccountId::new(id)
}

fn draining(
    account: &AccountId,
    reset: Option<UnixMillis>,
) -> BTreeMap<AccountId, QuotaObservation> {
    BTreeMap::from([(account.clone(), QuotaObservation::Draining(reset))])
}

mod persistence;

#[test]
fn resume_attempt_only_mutates_the_waiting_entry_it_selected() {
    let account = account("account");
    let thread = ThreadId::new("thread");
    let mut runtime = RotationRuntime::default();
    runtime.waiting(&thread, UnixMillis::new(1));
    let selected = runtime.waiting_threads()[0].clone();

    assert!(runtime.resumed_waiting(&selected, &account, UnixMillis::new(2)));
    assert!(runtime.waiting(&thread, UnixMillis::new(2)));
    let newer = runtime.waiting_threads()[0].clone();
    assert!(!runtime.resumed_waiting(&selected, &account, UnixMillis::new(3)));
    assert!(runtime
        .waiting_after_attempt(
            &selected,
            super::WaitingId::for_test("replacement"),
            UnixMillis::new(3),
        )
        .is_none());
    assert_eq!(runtime.waiting_threads(), &[newer]);
}

#[test]
fn failed_attempt_replaces_only_its_original_waiting_identity() {
    let thread = ThreadId::new("thread");
    let replacement = WaitingId::for_test("replacement");
    let mut runtime = RotationRuntime::default();
    runtime.waiting(&thread, UnixMillis::new(1));
    let selected = runtime.waiting_threads()[0].clone();

    let queued = runtime
        .waiting_after_attempt(&selected, replacement.clone(), UnixMillis::new(2))
        .unwrap();

    assert_eq!(queued.waiting_id, replacement);
    assert_eq!(runtime.waiting_threads(), &[queued]);
}

#[test]
fn legacy_waiting_identity_is_stable_and_new_enqueues_are_unique() {
    let legacy = r#"{"threadId":"thread","since":7}"#;
    let first: WaitingThread = serde_json::from_str(legacy).unwrap();
    let second: WaitingThread = serde_json::from_str(legacy).unwrap();
    assert_eq!(first.waiting_id, second.waiting_id);

    let one = WaitingThread::new(ThreadId::new("thread"), UnixMillis::new(7));
    let two = WaitingThread::new(ThreadId::new("thread"), UnixMillis::new(7));
    assert_ne!(one.waiting_id, two.waiting_id);
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
    assert!(settings.set_included(&b, false));
    assert!(settings.excluded().contains(&b));
}

#[test]
fn selection_honors_priority_and_skips_only_currently_unavailable_accounts() {
    let a = account("a");
    let b = account("b");
    let c = account("c");
    let discovered = [a.clone(), b.clone(), c.clone()];
    let mut settings = RotationSettings::default();
    settings.reconcile(&discovered);
    settings.set_enabled(true);
    settings.move_to(&c, 0);

    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&discovered, UnixMillis::new(0));
    runtime.block_admission(
        &c,
        BlockWindow::known(UnixMillis::new(20)),
        UnixMillis::new(1),
    );
    runtime.auth_failed(&a, UnixMillis::new(2));

    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(10)),
        Some(b.clone())
    );
    runtime.block_admission(
        &b,
        BlockWindow::known(UnixMillis::new(20)),
        UnixMillis::new(3),
    );
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
fn rejected_credential_quarantine_survives_discovery_omission_and_restart_without_secrets() {
    let a = account("omitted");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&a), UnixMillis::new(1));
    assert!(runtime.auth_failed_for_credential(
        &a,
        UnixMillis::new(2),
        Some("sha256:non-secret-fingerprint")
    ));

    assert!(!runtime.reconcile(&[], UnixMillis::new(3)));
    let bytes = serde_json::to_vec(&runtime).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("raw-provider-token"));
    let mut restarted: RotationRuntime = serde_json::from_slice(&bytes).unwrap();
    restarted.normalize().unwrap();

    assert_eq!(
        restarted.accounts()[&a].availability(UnixMillis::new(i64::MAX)),
        AccountAvailability::NeedsSignIn
    );
    let failure = restarted.auth_failure(&a).unwrap();
    assert!(restarted.sign_in_restored_by_proof(&a, failure, "sha256:new-credential-fingerprint"));
    assert!(restarted.credential_was_rejected(&a, "sha256:non-secret-fingerprint"));

    assert!(restarted.auth_failed_for_credential(
        &a,
        UnixMillis::new(4),
        Some("sha256:second-rejected-fingerprint")
    ));
    let later_failure = restarted.auth_failure(&a).unwrap();
    assert!(!restarted.sign_in_restored_by_proof(
        &a,
        later_failure,
        "sha256:non-secret-fingerprint"
    ));
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
fn runtime_tracks_threads_waiting_and_metadata_events_idempotently() {
    let a = account("a");
    let b = account("b");
    let thread = ThreadId::new("thread-1");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[a.clone(), b.clone()], UnixMillis::new(0));
    runtime.heartbeat(UnixMillis::new(1));
    runtime
        .connection_opened(&a, &thread, UnixMillis::new(2))
        .unwrap();
    runtime.rotated(&thread, &a, &b, UnixMillis::new(3));
    assert!(runtime.block_admission(
        &a,
        BlockWindow::known(UnixMillis::new(50)),
        UnixMillis::new(4)
    ));
    assert!(!runtime.block_admission(
        &a,
        BlockWindow::known(UnixMillis::new(50)),
        UnixMillis::new(4)
    ));
    assert!(runtime.auth_failed(&b, UnixMillis::new(5)));
    assert!(!runtime.auth_failed(&b, UnixMillis::new(5)));
    assert!(runtime.waiting(&thread, UnixMillis::new(6)));
    assert!(!runtime.waiting(&thread, UnixMillis::new(6)));
    assert!(runtime.resumed(&thread, &b, UnixMillis::new(7)));
    assert!(!runtime.resumed(&thread, &b, UnixMillis::new(7)));

    assert_eq!(runtime.health(), RouterHealth::Healthy);
    assert_eq!(runtime.heartbeat_at(), Some(UnixMillis::new(1)));
    assert_eq!(runtime.active_threads(&a), 1);
    assert!(runtime.connection_closed(&a, &thread, UnixMillis::new(8)));
    assert!(!runtime.connection_closed(&a, &thread, UnixMillis::new(9)));
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
fn live_count_is_unique_per_thread_and_ignores_follow_up_retention() {
    let account = account("a");
    let first = ThreadId::new("thread-1");
    let second = ThreadId::new("thread-2");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .connection_opened(&account, &first, UnixMillis::new(1))
        .unwrap();
    runtime
        .connection_opened(&account, &first, UnixMillis::new(2))
        .unwrap();
    runtime
        .connection_opened(&account, &second, UnixMillis::new(3))
        .unwrap();
    assert_eq!(runtime.active_threads(&account), 2);

    assert!(runtime.connection_closed(&account, &first, UnixMillis::new(4)));
    assert!(runtime.connection_continues(&account, &first, UnixMillis::new(5)));
    assert_eq!(runtime.active_threads(&account), 1);

    runtime
        .connection_opened(&account, &first, UnixMillis::new(6))
        .unwrap();
    assert!(runtime.connection_closed(&account, &first, UnixMillis::new(7)));
    assert_eq!(runtime.active_threads(&account), 1);

    runtime.thread_attached(&account, &second).unwrap();
    assert!(runtime.connection_continues(&account, &second, UnixMillis::new(8)));
    assert!(runtime.thread_detached(&account, &second));
    assert_eq!(runtime.active_threads(&account), 0);
    runtime
        .connection_opened(&account, &second, UnixMillis::new(9))
        .unwrap();
    assert!(runtime.connection_closed(&account, &second, UnixMillis::new(10)));
    assert_eq!(runtime.active_threads(&account), 0);
}

#[test]
fn confirmed_banked_reset_clears_the_old_hard_block() {
    let account = account("a");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.block_admission(
        &account,
        BlockWindow::known(UnixMillis::new(10_000)),
        UnixMillis::new(1),
    );

    assert!(runtime.banked_reset_consumed(&account));
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(2)),
        AccountAvailability::Available
    );
    assert!(!runtime.banked_reset_consumed(&account));
}

#[test]
fn overlapping_connections_keep_a_thread_attached_until_the_last_one_closes() {
    let account = account("a");
    let thread = ThreadId::new("thread");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &thread).unwrap();
    runtime.thread_attached(&account, &thread).unwrap();
    runtime.thread_detached(&account, &thread);
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(10),
    );

    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(20)));
    runtime.thread_detached(&account, &thread);
    runtime.apply_quota_observations(
        &BTreeMap::from([(account.clone(), QuotaObservation::ObservedAvailable)]),
        UnixMillis::new(21),
    );
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(22),
    );
    assert!(!runtime.can_drain(&account, &thread, UnixMillis::new(23)));
}

#[test]
fn attached_threads_survive_a_store_round_trip_for_draining_workers() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let account = account("a");
    let thread = ThreadId::new("long-running-tool-chain");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &thread).unwrap();
    store.save(&runtime).unwrap();

    let mut reloaded = store.load().unwrap();
    reloaded.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(10),
    );

    assert!(reloaded.can_drain(&account, &thread, UnixMillis::new(20)));
    assert!(fs::read_to_string(store.path())
        .unwrap()
        .contains("attachedThreads"));
}

#[test]
fn runtime_transactions_from_two_router_generations_do_not_lose_updates() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let start = std::sync::Arc::new(std::sync::Barrier::new(3));
    let workers = ["worker-a", "worker-b"].map(|thread| {
        let store = store.clone();
        let start = start.clone();
        std::thread::spawn(move || {
            start.wait();
            store
                .update(|runtime| {
                    let changed = runtime.waiting(&ThreadId::new(thread), UnixMillis::new(1));
                    ((), changed)
                })
                .unwrap();
        })
    });
    start.wait();
    for worker in workers {
        worker.join().unwrap();
    }

    let runtime = store.load().unwrap();
    let mut waiting = runtime
        .waiting_threads()
        .iter()
        .map(|entry| entry.thread_id.as_str())
        .collect::<Vec<_>>();
    waiting.sort_unstable();
    assert_eq!(waiting, ["worker-a", "worker-b"]);
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
fn incident_observability_reserves_history_from_routing_churn() {
    let mut runtime = RotationRuntime::default();
    let primary = account("account");
    for at in 0..20 {
        runtime.usage_limited(
            &primary,
            UsageLimitIncident::new(
                Some(ThreadId::new(format!("incident-{at}"))),
                Some("gpt-5.6-sol"),
                UsageLimitTier::new(Some("priority"), UsageLimitTierOrigin::ToksForcedFast),
                UsageLimitPhase::WebSocketFrame,
                UsageLimitEvidence::from_upstream(
                    UsageLimitClassification::ErrorMessage,
                    None,
                    Some("turn.failed"),
                    None,
                    None,
                    format!("usage-limit-{at}").as_bytes(),
                ),
            ),
            UnixMillis::new(at),
        );
    }
    for at in 20..120 {
        runtime.rotated(
            &ThreadId::new(format!("route-{at}")),
            &primary,
            &account("other"),
            UnixMillis::new(at),
        );
    }

    assert_eq!(runtime.events().len(), 100);
    assert_eq!(
        runtime
            .events()
            .iter()
            .filter(|event| matches!(&event.event, RotationEventKind::UsageLimited { .. }))
            .count(),
        20
    );
    assert!(runtime
        .events()
        .iter()
        .any(|event| event.at == UnixMillis::new(0)));
    assert_eq!(runtime.events().front().unwrap().at, UnixMillis::new(119));
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
    assert_eq!(fs::read_dir(state_dir).unwrap().count(), 4);
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
        for path in [
            settings_store.path().to_owned(),
            runtime_store.path().to_owned(),
            state_dir.join("settings.json.lock"),
            state_dir.join("runtime.json.lock"),
        ] {
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
