use super::pending::{AbandonedStage, Pending, HANDOFF_SETTLE_TIMEOUT};
use crate::codex_router::handoff::{
    Control, GenerationId as WireGenerationId, Received, WorkerInstanceId,
};
use crate::codex_router::host::process::test_fixtures::{
    accepting_worker, channel_pair, fixture, tcp_pair,
};

#[tokio::test]
async fn periodic_retry_converges_after_lost_prepare_and_commit_acks() {
    let (_directory, mut coordinator, peer, generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let Received::Connection(first, _first_copy) = peer.receive().await.unwrap() else {
        panic!("expected initial descriptor")
    };
    assert!(!first.duplicate);

    coordinator.retry_all_pending().await;
    let Received::Connection(retried_prepare, _second_copy) = peer.receive().await.unwrap() else {
        panic!("expected prepare retry")
    };
    assert_eq!(retried_prepare.handoff_id, first.handoff_id);
    assert!(retried_prepare.duplicate);
    coordinator
        .handle_message(
            generation,
            Control::ConnectionAck {
                handoff_id: first.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionCommitted { handoff_id })
            if handoff_id == first.handoff_id
    ));

    coordinator.retry_all_pending().await;
    let Received::Connection(retried_commit, _third_copy) = peer.receive().await.unwrap() else {
        panic!("expected commit retry")
    };
    assert_eq!(retried_commit.handoff_id, first.handoff_id);
    assert!(retried_commit.duplicate);
    coordinator
        .handle_message(
            generation,
            Control::ConnectionCommitAck {
                handoff_id: first.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == first.handoff_id
    ));
    assert!(!coordinator.pending.is_empty());
    coordinator
        .handle_message(
            generation,
            Control::ConnectionFinalizedAck {
                handoff_id: first.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(coordinator.pending.is_empty());
}

#[tokio::test]
async fn lost_finalization_ack_is_retried_after_same_epoch_reconnect() {
    let (_directory, mut coordinator, first_peer, generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let Received::Connection(connection, _copy) = first_peer.receive().await.unwrap() else {
        panic!("expected descriptor")
    };
    coordinator
        .handle_message(
            generation,
            Control::ConnectionCommitAck {
                handoff_id: connection.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        first_peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == connection.handoff_id
    ));

    let (replacement, replacement_peer) = channel_pair();
    coordinator.workers.replace(
        generation,
        accepting_worker(replacement, 1, WorkerInstanceId::new(1).unwrap()),
    );
    coordinator.retry_pending(generation).await.unwrap();
    assert!(matches!(
        replacement_peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == connection.handoff_id
    ));
    coordinator
        .handle_message(
            generation,
            Control::ConnectionFinalizedAck {
                handoff_id: connection.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(coordinator.pending.is_empty());
}

#[tokio::test]
async fn in_flight_cap_closes_and_reopens_admission_after_finalization() {
    let (_directory, mut coordinator, _peer, generation) = fixture().await;
    coordinator.pending = Pending::with_capacity(2).unwrap();
    let (_client_a, server_a) = tcp_pair().await;
    let (_client_b, server_b) = tcp_pair().await;
    let first = coordinator
        .pending
        .insert(WireGenerationId::new(generation.get()), server_a)
        .unwrap();
    let second = coordinator
        .pending
        .insert(WireGenerationId::new(generation.get()), server_b)
        .unwrap();

    assert!(!coordinator.accepts_clients());
    let (_client_c, server_c) = tcp_pair().await;
    assert!(coordinator
        .pending
        .insert(WireGenerationId::new(generation.get()), server_c)
        .is_err());
    assert!(coordinator
        .pending
        .begin_finalizing(WireGenerationId::new(generation.get()), first));
    assert!(!coordinator.accepts_clients());
    assert!(coordinator
        .pending
        .acknowledge_finalized(WireGenerationId::new(generation.get()), first));
    assert!(coordinator.accepts_clients());
    assert!(coordinator
        .pending
        .remove(WireGenerationId::new(generation.get()), second));
}

#[tokio::test]
async fn stalled_finalization_retry_cannot_monopolize_the_coordinator() {
    let (_directory, mut coordinator, _peer, generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    let id = coordinator
        .pending
        .insert(WireGenerationId::new(generation.get()), server)
        .unwrap();
    assert!(coordinator
        .pending
        .begin_finalizing(WireGenerationId::new(generation.get()), id));
    coordinator
        .workers
        .channel_for(generation)
        .unwrap()
        .fill_send_buffer();

    let started = tokio::time::Instant::now();
    coordinator.retry_all_pending().await;

    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    assert!(!coordinator.pending.is_empty());
}

#[tokio::test]
async fn shutdown_deadline_bounds_an_unacknowledged_handoff() {
    let (_directory, mut coordinator, _peer, _generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let now = tokio::time::Instant::now();

    assert!(!super::shutdown_complete(Some(now), &coordinator, now));
    assert!(super::shutdown_complete(
        Some(now),
        &coordinator,
        now + crate::codex_router::host::COORDINATOR_SHUTDOWN_DRAIN_TIMEOUT,
    ));
}

#[tokio::test]
async fn an_unsettled_handoff_is_abandoned_and_its_slot_reclaimed() {
    let (_directory, mut coordinator, peer, _generation) = fixture().await;
    let (client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let Received::Connection(delivered, _copy) = peer.receive().await.unwrap() else {
        panic!("expected initial descriptor")
    };
    assert!(!coordinator.pending.is_empty());

    // The client gives up before the worker commits, so no acknowledgement will
    // ever arrive to retire the slot or release the descriptor behind it.
    drop(client);
    let abandoned = coordinator.pending.reap_expired(
        tokio::time::Instant::now() + HANDOFF_SETTLE_TIMEOUT,
        HANDOFF_SETTLE_TIMEOUT,
    );

    assert_eq!(abandoned.len(), 1);
    assert_eq!(abandoned[0].id, delivered.handoff_id);
    assert_eq!(abandoned[0].stage, AbandonedStage::Preparing);
    assert!(coordinator.pending.is_empty());
}

#[tokio::test]
async fn a_handoff_still_inside_the_settle_window_is_left_alone() {
    let (_directory, mut coordinator, peer, _generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let Received::Connection(_delivered, _copy) = peer.receive().await.unwrap() else {
        panic!("expected initial descriptor")
    };

    let abandoned = coordinator
        .pending
        .reap_expired(tokio::time::Instant::now(), HANDOFF_SETTLE_TIMEOUT);

    assert!(abandoned.is_empty());
    assert!(!coordinator.pending.is_empty());
}

#[tokio::test]
async fn a_handoff_reaped_mid_commit_still_tells_the_worker_to_forget_it() {
    let (_directory, mut coordinator, peer, generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let Received::Connection(connection, _copy) = peer.receive().await.unwrap() else {
        panic!("expected descriptor")
    };
    coordinator
        .handle_message(
            generation,
            Control::ConnectionAck {
                handoff_id: connection.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionCommitted { handoff_id })
            if handoff_id == connection.handoff_id
    ));

    // The commit acknowledgement never arrives, so the reaper abandons the
    // handoff — but the worker may have committed (tombstone) or still be
    // parking the descriptor, so it must be told to forget the handoff.
    coordinator
        .reap_stale_handoffs(tokio::time::Instant::now() + HANDOFF_SETTLE_TIMEOUT)
        .await;

    assert!(coordinator.pending.is_empty());
    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == connection.handoff_id
    ));
}

#[tokio::test]
async fn a_reaped_finalization_still_tells_the_worker_to_drop_its_tombstone() {
    let (_directory, mut coordinator, peer, generation) = fixture().await;
    let (_client, server) = tcp_pair().await;
    coordinator.handoff(server).await.unwrap();
    let Received::Connection(connection, _copy) = peer.receive().await.unwrap() else {
        panic!("expected descriptor")
    };
    coordinator
        .handle_message(
            generation,
            Control::ConnectionCommitAck {
                handoff_id: connection.handoff_id,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == connection.handoff_id
    ));

    // The finalization acknowledgement never arrives, so the reaper abandons
    // the handoff — but the worker committed, so it must still be told to
    // release its idempotency tombstone.
    coordinator
        .reap_stale_handoffs(tokio::time::Instant::now() + HANDOFF_SETTLE_TIMEOUT)
        .await;

    assert!(coordinator.pending.is_empty());
    assert!(matches!(
        peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == connection.handoff_id
    ));
    // The worker's late acknowledgement of that notification is a no-op.
    coordinator
        .handle_message(
            generation,
            Control::ConnectionFinalizedAck {
                handoff_id: connection.handoff_id,
            },
        )
        .await
        .unwrap();
}
