use futures_util::FutureExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::super::channel::{AsyncChannel, AsyncListener};
use super::super::paths::HostPaths;
use super::super::worker::{run_with, Service};
use super::core::{Coordinator, WorkerSlot};
use super::events::spawn_reader;
use crate::codex_router::handoff::{
    Connection, Control, GenerationId as WireGenerationId, HandoffChannel, HandoffId,
    HandoffListener, Received, WorkerInstanceId,
};
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId,
};

#[tokio::test]
async fn stale_connection_ack_never_commits_an_unknown_handoff() {
    let mut fixture = CoordinatorFixture::new().await;
    fixture
        .coordinator
        .handle_message(
            fixture.generation,
            Control::ConnectionAck {
                handoff_id: HandoffId::new(44, 1),
            },
        )
        .await
        .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(75), fixture.peer.receive())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn normal_ack_order_commits_without_retransferring_the_descriptor() {
    let mut fixture = CoordinatorFixture::new().await;
    let (_client, server) = tcp_pair().await;
    fixture.coordinator.handoff(server).await.unwrap();
    let Received::Connection(connection, _worker_copy) = fixture.peer.receive().await.unwrap()
    else {
        panic!("expected initial connection transfer");
    };
    let (events, mut worker_events) = tokio::sync::mpsc::unbounded_channel();
    let coordinator_channel = fixture
        .coordinator
        .workers
        .get(&fixture.generation)
        .unwrap()
        .channel
        .clone();
    spawn_reader(fixture.generation, 1, coordinator_channel, events.clone());
    fixture
        .peer
        .send_control(&Control::ConnectionAck {
            handoff_id: connection.handoff_id,
        })
        .await
        .unwrap();
    fixture
        .coordinator
        .worker_event(receive_event(&mut worker_events).await, events)
        .await
        .unwrap();

    assert!(matches!(
        fixture.peer.receive().await.unwrap(),
        Received::Control(Control::ConnectionCommitted { handoff_id })
            if handoff_id == connection.handoff_id
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(75), fixture.peer.receive())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn graceful_shutdown_waits_through_prepared_worker_reconnect() {
    let calls = Arc::new(AtomicUsize::new(0));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.sock");
    let listener = AsyncListener::new(HandoffListener::bind(&path).unwrap()).unwrap();
    let generation = GenerationId::from_raw(1);
    let worker = tokio::spawn(run_with(generation, path, echo_service(calls.clone())));
    let first = accept_worker(&listener, generation, 0, 51).await;
    activate(&first, generation).await;
    let mut fixture = CoordinatorFixture::with_channel(first.clone(), generation).await;
    let (mut client, server) = tcp_pair().await;
    fixture.coordinator.handoff(server).await.unwrap();
    let ack = first.receive().await.unwrap();
    let Received::Control(Control::ConnectionAck { handoff_id }) = ack else {
        panic!("expected worker prepare acknowledgement");
    };
    assert!(!fixture.coordinator.pending.is_empty());
    assert!(!super::shutdown_complete(
        Some(tokio::time::Instant::now()),
        &fixture.coordinator,
        tokio::time::Instant::now(),
    ));

    drop(fixture.coordinator.workers.remove(&generation));
    drop(first);
    let adopted = accept_worker(&listener, generation, 0, 51).await;
    activate(&adopted, generation).await;
    fixture.insert_worker(adopted.clone(), 2);
    fixture.coordinator.retry_pending(generation).await.unwrap();
    assert!(matches!(
        receive(&adopted).await,
        Received::Control(Control::ConnectionAck { handoff_id: found }) if found == handoff_id
    ));
    fixture
        .coordinator
        .handle_message(generation, Control::ConnectionAck { handoff_id })
        .await
        .unwrap();
    assert!(matches!(
        receive(&adopted).await,
        Received::Control(Control::ConnectionCommitAck { handoff_id: found }) if found == handoff_id
    ));
    fixture
        .coordinator
        .handle_message(generation, Control::ConnectionCommitAck { handoff_id })
        .await
        .unwrap();

    assert!(matches!(
        receive(&adopted).await,
        Received::Control(Control::ConnectionFinalizedAck { handoff_id: found })
            if found == handoff_id
    ));
    fixture
        .coordinator
        .handle_message(generation, Control::ConnectionFinalizedAck { handoff_id })
        .await
        .unwrap();

    assert!(fixture.coordinator.pending.is_empty());
    assert!(super::shutdown_complete(
        Some(tokio::time::Instant::now()),
        &fixture.coordinator,
        tokio::time::Instant::now(),
    ));
    round_trip(&mut client, b"resumed").await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    stop_worker(&adopted, generation, client).await;
    worker.await.unwrap().unwrap();
}

#[tokio::test]
async fn committed_id_is_idempotent_after_lost_ack_and_reconnect() {
    let calls = Arc::new(AtomicUsize::new(0));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.sock");
    let listener = AsyncListener::new(HandoffListener::bind(&path).unwrap()).unwrap();
    let generation = GenerationId::from_raw(19);
    let worker = tokio::spawn(run_with(generation, path, echo_service(calls.clone())));
    let first = accept_worker(&listener, generation, 0, 71).await;
    activate(&first, generation).await;
    first
        .send_control(&Control::ConnectionCommitted {
            handoff_id: HandoffId::new(71, 2),
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(75), first.receive())
            .await
            .is_err()
    );
    let (mut client, server) = tcp_pair().await;
    let id = HandoffId::new(71, 3);
    prepare(&first, id, &server, false).await;
    commit(&first, id).await;
    first
        .send_control(&Control::ConnectionCommitted { handoff_id: id })
        .await
        .unwrap();
    assert!(matches!(
        receive(&first).await,
        Received::Control(Control::ConnectionCommitAck { handoff_id }) if handoff_id == id
    ));
    drop(first);

    let adopted = accept_worker(&listener, generation, 1, 71).await;
    activate(&adopted, generation).await;
    adopted
        .send_connection(
            Connection {
                handoff_id: id,
                duplicate: true,
            },
            &server,
        )
        .await
        .unwrap();
    assert!(matches!(
        receive(&adopted).await,
        Received::Control(Control::ConnectionCommitAck { handoff_id }) if handoff_id == id
    ));
    for _ in 0..2 {
        adopted
            .send_control(&Control::ConnectionFinalized { handoff_id: id })
            .await
            .unwrap();
        assert!(matches!(
            receive(&adopted).await,
            Received::Control(Control::ConnectionFinalizedAck { handoff_id })
                if handoff_id == id
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(75), adopted.receive())
            .await
            .is_err()
    );
    round_trip(&mut client, b"once").await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    drop(server);
    stop_worker(&adopted, generation, client).await;
    worker.await.unwrap().unwrap();
}

struct CoordinatorFixture {
    _directory: tempfile::TempDir,
    coordinator: Coordinator,
    peer: Arc<AsyncChannel>,
    generation: GenerationId,
}

impl CoordinatorFixture {
    async fn new() -> Self {
        let (coordinator_channel, peer, directory) = channel_pair();
        Self::build(directory, coordinator_channel, peer, None).await
    }

    async fn with_channel(channel: Arc<AsyncChannel>, generation: GenerationId) -> Self {
        let (_unused, peer, directory) = channel_pair();
        Self::build(directory, channel, peer, Some(generation)).await
    }

    async fn build(
        directory: tempfile::TempDir,
        coordinator_channel: Arc<AsyncChannel>,
        peer: Arc<AsyncChannel>,
        generation: Option<GenerationId>,
    ) -> Self {
        let executable = directory.path().join("router");
        std::fs::write(&executable, b"same-build").unwrap();
        let paths = HostPaths {
            executable,
            generations: directory.path().join("generations"),
            control: directory.path().join("unused.sock"),
            state: directory.path().join("state.json"),
        };
        let build = paths.build_id().unwrap();
        let (deployment, found_generation) = active_deployment(build);
        let generation = generation.unwrap_or(found_generation);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut coordinator = Coordinator::new(listener, paths, deployment).unwrap();
        coordinator.active = Some(generation);
        coordinator.workers.insert(
            generation,
            WorkerSlot {
                registration: 1,
                instance: WorkerInstanceId::new(1).unwrap(),
                ready: true,
                accepting: true,
                draining: false,
                pending_reconciled: true,
                channel: coordinator_channel,
            },
        );
        Self {
            _directory: directory,
            coordinator,
            peer,
            generation,
        }
    }

    fn insert_worker(&mut self, channel: Arc<AsyncChannel>, registration: u64) {
        self.coordinator.workers.insert(
            self.generation,
            WorkerSlot {
                registration,
                instance: WorkerInstanceId::new(1).unwrap(),
                ready: true,
                accepting: true,
                draining: false,
                pending_reconciled: true,
                channel,
            },
        );
    }
}

fn channel_pair() -> (Arc<AsyncChannel>, Arc<AsyncChannel>, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pair.sock");
    let listener = HandoffListener::bind(&path).unwrap();
    let peer = HandoffChannel::connect(&path).unwrap();
    let coordinator = listener.accept().unwrap();
    (
        Arc::new(AsyncChannel::new(coordinator).unwrap()),
        Arc::new(AsyncChannel::new(peer).unwrap()),
        directory,
    )
}

async fn accept_worker(
    listener: &AsyncListener,
    generation: GenerationId,
    active: u64,
    epoch: u64,
) -> Arc<AsyncChannel> {
    let channel = listener.accept().await.unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::WorkerHello { generation: found, .. })
            if found.raw() == generation.get()
    ));
    channel
        .send_control(&Control::CoordinatorHello { epoch })
        .await
        .unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::Ready { generation: found })
            if found.raw() == generation.get()
    ));
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::ConnectionsObserved { generation: found, active: found_active })
            if found.raw() == generation.get() && found_active == active
    ));
    channel
}

async fn receive(channel: &AsyncChannel) -> Received {
    tokio::time::timeout(Duration::from_secs(2), channel.receive())
        .await
        .expect("timed out waiting for worker control message")
        .unwrap()
}

async fn receive_event(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<super::events::WorkerEvent>,
) -> super::events::WorkerEvent {
    tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("timed out waiting for coordinator worker event")
        .expect("coordinator worker event channel closed")
}

async fn activate(channel: &AsyncChannel, generation: GenerationId) {
    channel
        .send_control(&Control::Activate {
            generation: WireGenerationId::new(generation.get()),
        })
        .await
        .unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::Accepting { generation: found })
            if found.raw() == generation.get()
    ));
}

async fn prepare(
    channel: &AsyncChannel,
    id: HandoffId,
    stream: &tokio::net::TcpStream,
    duplicate: bool,
) {
    channel
        .send_connection(
            Connection {
                handoff_id: id,
                duplicate,
            },
            stream,
        )
        .await
        .unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::ConnectionAck { handoff_id }) if handoff_id == id
    ));
}

async fn commit(channel: &AsyncChannel, id: HandoffId) {
    channel
        .send_control(&Control::ConnectionCommitted { handoff_id: id })
        .await
        .unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::ConnectionCommitAck { handoff_id }) if handoff_id == id
    ));
}

async fn stop_worker(
    channel: &AsyncChannel,
    generation: GenerationId,
    client: tokio::net::TcpStream,
) {
    channel
        .send_control(&Control::Drain {
            generation: WireGenerationId::new(generation.get()),
        })
        .await
        .unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::AdmissionsPaused { generation: found })
            if found.raw() == generation.get()
    ));
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::ConnectionsObserved { active: 1, .. })
    ));
    drop(client);
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::ConnectionsObserved { active: 0, .. })
    ));
}

async fn tcp_pair() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = tokio::spawn(tokio::net::TcpStream::connect(address));
    let (server, _) = listener.accept().await.unwrap();
    (client.await.unwrap().unwrap(), server)
}

async fn round_trip(stream: &mut tokio::net::TcpStream, message: &[u8]) {
    stream.write_all(message).await.unwrap();
    let mut found = vec![0; message.len()];
    stream.read_exact(&mut found).await.unwrap();
    assert_eq!(found, message);
}

fn echo_service(calls: Arc<AtomicUsize>) -> Service {
    Arc::new(move |mut stream, lifetime| {
        calls.fetch_add(1, Ordering::AcqRel);
        async move {
            let _lifetime = lifetime;
            let mut bytes = [0_u8; 128];
            while let Ok(size) = stream.read(&mut bytes).await {
                if size == 0 || stream.write_all(&bytes[..size]).await.is_err() {
                    break;
                }
            }
        }
        .boxed()
    })
}

fn active_deployment(build: BuildId) -> (DeploymentState, GenerationId) {
    let mut state = DeploymentState::default();
    let DeployPlan::StageTarget { target, .. } = state.plan_deploy(build).unwrap() else {
        panic!("expected staged generation");
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
