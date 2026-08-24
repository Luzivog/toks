use super::super::channel::AsyncChannel;
use super::super::paths::HostPaths;
use super::core::{Coordinator, WorkerSlot};
use super::pending::{Pending, HANDOFF_SETTLE_TIMEOUT};
use crate::codex_router::handoff::{
    Control, GenerationId as WireGenerationId, HandoffChannel, HandoffListener, Received,
    WorkerInstanceId,
};
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId,
};
use std::sync::Arc;

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
            generation.get(),
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
            generation.get(),
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
            generation.get(),
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
            generation.get(),
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
    coordinator
        .workers
        .insert(generation.get(), worker_slot(replacement));
    coordinator.retry_pending(generation.get()).await.unwrap();
    assert!(matches!(
        replacement_peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionFinalized { handoff_id })
            if handoff_id == connection.handoff_id
    ));
    coordinator
        .handle_message(
            generation.get(),
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
        .get(&generation.get())
        .unwrap()
        .channel
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
    assert_eq!(abandoned[0].stage, "preparing");
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

async fn fixture() -> (
    tempfile::TempDir,
    Coordinator,
    Arc<AsyncChannel>,
    GenerationId,
) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("router");
    std::fs::write(&executable, b"same-build").unwrap();
    let paths = HostPaths {
        executable,
        generations: directory.path().join("generations"),
        control: directory.path().join("unused.sock"),
        state: directory.path().join("state.json"),
    };
    let (deployment, generation) = active_deployment(paths.build_id().unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut coordinator = Coordinator::new(listener, paths, deployment).unwrap();
    let (channel, peer) = channel_pair();
    coordinator.active = Some(generation);
    coordinator
        .workers
        .insert(generation.get(), worker_slot(channel));
    (directory, coordinator, peer, generation)
}

fn worker_slot(channel: Arc<AsyncChannel>) -> WorkerSlot {
    WorkerSlot {
        registration: 1,
        instance: WorkerInstanceId::new(1).unwrap(),
        ready: true,
        accepting: true,
        draining: false,
        pending_reconciled: true,
        channel,
    }
}

fn channel_pair() -> (Arc<AsyncChannel>, Arc<AsyncChannel>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pair.sock");
    let listener = HandoffListener::bind(&path).unwrap();
    let peer = HandoffChannel::connect(&path).unwrap();
    let coordinator = listener.accept().unwrap();
    (
        Arc::new(AsyncChannel::new(coordinator).unwrap()),
        Arc::new(AsyncChannel::new(peer).unwrap()),
    )
}

async fn tcp_pair() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = tokio::spawn(tokio::net::TcpStream::connect(address));
    let (server, _) = listener.accept().await.unwrap();
    (client.await.unwrap().unwrap(), server)
}

fn active_deployment(build: BuildId) -> (DeploymentState, GenerationId) {
    let mut state = DeploymentState::default();
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(build).unwrap() else {
        panic!("expected staged generation")
    };
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
        DeploymentEvent::TargetAccepting { target },
    ] {
        state.reconcile(event).unwrap();
    }
    (state, target)
}
