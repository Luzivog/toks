use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::codex_router::proxy::lease::{StreamLease, TerminalOwnership, ThreadAttachment};
use crate::codex_router::proxy::protocol::websocket_usage_block;
use crate::rotation::{
    AccountAvailability, RotationEventKind, TaskActivityStore, ThreadId, UnixMillis,
    UsageLimitPhase, UsageLimitTier,
};

use super::Engines;

#[test]
fn committed_hard_handoff_survives_guard_loss_and_reconciliation() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let worker = engines.worker(7, 701);
    let exhausted = AccountId::new("a");
    let replacement = AccountId::new("b");
    let thread = ThreadId::new("atomic-hard-handoff");
    let activity = TaskActivityStore::for_data_dir(engines._directory.path());
    activity
        .reconcile_expected_workers(&BTreeMap::from([(7, 701)]))
        .unwrap();
    let mut stream = StreamLease::open(worker.clone(), &exhausted, &thread, None).unwrap();
    let mut attachment = ThreadAttachment::open(worker.clone(), &exhausted, &thread, None).unwrap();
    assert_eq!(
        activity.load().unwrap().active_task_rows().unwrap().len(),
        1
    );
    let reset = UnixMillis::new(i64::MAX - 1);
    let block = websocket_usage_block(&format!(
        r#"{{"type":"error","error":{{"type":"usage_limit_reached","resets_at":{}}}}}"#,
        reset.get()
    ))
    .unwrap();

    TerminalOwnership::take(&mut stream, &mut attachment)
        .unwrap()
        .commit_delivered_hard_limit(
            block.resets_at,
            block.incident(
                Some(thread.clone()),
                Some("gpt-5.6-sol"),
                UsageLimitTier::client(Some("default")),
                UsageLimitPhase::WebSocketFrame,
            ),
        )
        .unwrap();
    assert!(activity
        .load()
        .unwrap()
        .active_task_rows()
        .unwrap()
        .is_empty());

    let committed = engines.store.load().unwrap();
    assert_eq!(committed.in_flight_count(&exhausted), 0);
    assert!(matches!(
        committed.accounts()[&exhausted].availability(UnixMillis::now()),
        AccountAvailability::Blocked { .. }
    ));
    assert_eq!(committed.waiting_threads().len(), 1);
    assert_eq!(committed.waiting_threads()[0].thread_id, thread);
    let restarted = engines.worker(7, 702);
    restarted.reconcile_owned_connections().unwrap();
    restarted.waiting(&thread).unwrap();

    let runtime = engines.store.load().unwrap();
    assert_eq!(runtime.in_flight_count(&exhausted), 0);
    assert!(matches!(
        runtime.accounts()[&exhausted].availability(UnixMillis::now()),
        AccountAvailability::Blocked { .. }
    ));
    assert_eq!(runtime.waiting_threads().len(), 1);
    assert_eq!(runtime.waiting_threads()[0].thread_id, thread);
    assert_eq!(
        runtime
            .events()
            .iter()
            .filter(|event| matches!(&event.event, RotationEventKind::Waiting { .. }))
            .count(),
        1
    );
    assert_eq!(
        restarted
            .eligible_account_for_thread(&runtime.waiting_threads()[0].thread_id)
            .unwrap(),
        Some(replacement)
    );
}
