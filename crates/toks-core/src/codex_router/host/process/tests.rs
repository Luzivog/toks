use futures_util::FutureExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::activated::Activation;
use super::channel::{AsyncChannel, AsyncListener};
use super::coordinator::{trusted_peer, Coordinator};
use super::paths::{load_state, HostPaths};
use super::worker::{run_with, Service};
use crate::codex_router::handoff::{
    Connection, Control, GenerationId as WireGenerationId, HandoffId, HandoffListener, Received,
};
use crate::codex_router::host::model::ActivationPhase;
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId, GenerationStatus,
};

#[test]
fn activation_requires_one_named_listener_for_this_pid() {
    let activation = Activation::new(
        &[
            ("LISTEN_PID", "71"),
            ("LISTEN_FDS", "1"),
            ("LISTEN_FDNAMES", "router"),
        ],
        71,
    );
    assert_eq!(activation.descriptor().unwrap(), 3);
}

#[test]
fn activation_and_peer_checks_reject_foreign_or_ambiguous_inputs() {
    for values in [
        activation_values("72", "1", "router"),
        activation_values("71", "2", "router"),
        activation_values("71", "1", "other"),
    ] {
        assert!(Activation::new(&values, 71).descriptor().is_err());
    }
    assert!(!trusted_peer(1001, 1000));
    assert!(trusted_peer(1000, 1000));
}

#[tokio::test]
async fn deployment_phases_advance_only_after_worker_acknowledgements() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"candidate-build").unwrap();
    let (mut deployment, previous) = active_deployment("old-build");
    let paths = test_paths(directory.path(), executable);
    let listener = test_listener().await;
    let mut coordinator = Coordinator::new(listener, paths, deployment.clone()).unwrap();
    let target = staged_generation(&coordinator.deployment);
    coordinator
        .deployment
        .reconcile(DeploymentEvent::Prepared { target })
        .unwrap();
    assert_eq!(
        activation_phase(&coordinator.deployment),
        ActivationPhase::Prepared
    );

    coordinator
        .handle_message(
            previous.get(),
            Control::AdmissionsPaused {
                generation: WireGenerationId::new(previous.get()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        activation_phase(&coordinator.deployment),
        ActivationPhase::PreviousPaused
    );
    coordinator
        .handle_message(
            target.get(),
            Control::Accepting {
                generation: WireGenerationId::new(target.get()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        activation_phase(&coordinator.deployment),
        ActivationPhase::TargetAccepting
    );
    deployment = load_state(&coordinator.paths.state).unwrap();
    assert_eq!(
        activation_phase(&deployment),
        ActivationPhase::TargetAccepting
    );
}

#[tokio::test]
async fn candidate_stage_failure_keeps_previous_generation_active() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"candidate-build").unwrap();
    let (deployment, previous) = active_deployment("old-build");
    let paths = test_paths(directory.path(), executable);
    std::fs::write(&paths.generations, b"not-a-directory").unwrap();
    let listener = test_listener().await;
    let mut coordinator = Coordinator::new(listener, paths, deployment).unwrap();

    coordinator.advance().await.unwrap();

    assert_eq!(coordinator.active, Some(previous));
    assert!(coordinator
        .deployment
        .snapshot()
        .generations
        .iter()
        .any(|generation| generation.status == GenerationStatus::Failed));
}

#[tokio::test]
async fn live_connection_survives_control_loss_and_worker_adoption() {
    let harness = Harness::start(7, echo_service()).await;
    let first = harness.accept_worker(7, 0).await;
    activate(&first, 7).await;
    let (mut client, server) = tcp_pair().await;
    handoff(&first, HandoffId::new(4, 1), &server, false).await;
    drop(server);
    round_trip(&mut client, b"before").await;

    drop(first);
    round_trip(&mut client, b"during").await;
    let adopted = harness.accept_worker(7, 1).await;
    activate(&adopted, 7).await;
    round_trip(&mut client, b"after").await;

    drain(&adopted, 7, 1).await;
    drop(client);
    observed(&adopted, 7, 0).await;
    tokio::time::timeout(Duration::from_secs(2), harness.worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn duplicate_handoff_is_acknowledged_without_duplicate_service() {
    let calls = Arc::new(AtomicUsize::new(0));
    let harness = Harness::start(8, counted_service(calls.clone())).await;
    let channel = harness.accept_worker(8, 0).await;
    activate(&channel, 8).await;
    let (client, server) = tcp_pair().await;
    let id = HandoffId::new(9, 1);
    prepare_handoff(&channel, id, &server, false).await;
    prepare_handoff(&channel, id, &server, true).await;
    commit_handoff(&channel, id).await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    drop(server);
    drain(&channel, 8, 1).await;
    drop(client);
    observed(&channel, 8, 0).await;
}

#[tokio::test]
async fn inactive_worker_backpressures_until_activate_and_same_id_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let harness = Harness::start(9, counted_service(calls.clone())).await;
    let channel = harness.accept_worker(9, 0).await;
    let (client, server) = tcp_pair().await;
    let id = HandoffId::new(10, 1);
    channel
        .send_connection(
            Connection {
                handoff_id: id,
                duplicate: false,
            },
            &server,
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(75), channel.receive())
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);

    activate(&channel, 9).await;
    handoff(&channel, id, &server, true).await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    drop(server);
    drain(&channel, 9, 1).await;
    drop(client);
    observed(&channel, 9, 0).await;
}

#[tokio::test]
async fn activation_moves_new_connections_while_old_connection_survives() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.sock");
    let listener = AsyncListener::new(HandoffListener::bind(&path).unwrap()).unwrap();
    let a_calls = Arc::new(AtomicUsize::new(0));
    let b_calls = Arc::new(AtomicUsize::new(0));
    let a = tokio::spawn(run_with(
        GenerationId::from_raw(11),
        path.clone(),
        counted_echo_service(a_calls.clone()),
    ));
    let channel_a = accept_generation(&listener, 11, 0).await;
    activate(&channel_a, 11).await;
    let (mut client_a, server_a) = tcp_pair().await;
    handoff(&channel_a, HandoffId::new(12, 1), &server_a, false).await;
    drop(server_a);
    drain(&channel_a, 11, 1).await;

    let b = tokio::spawn(run_with(
        GenerationId::from_raw(12),
        path,
        counted_echo_service(b_calls.clone()),
    ));
    let channel_b = accept_generation(&listener, 12, 0).await;
    activate(&channel_b, 12).await;
    let (mut client_b, server_b) = tcp_pair().await;
    handoff(&channel_b, HandoffId::new(12, 2), &server_b, false).await;
    drop(server_b);

    round_trip(&mut client_a, b"old").await;
    round_trip(&mut client_b, b"new").await;
    assert_eq!(a_calls.load(Ordering::Acquire), 1);
    assert_eq!(b_calls.load(Ordering::Acquire), 1);
    drop(client_a);
    observed(&channel_a, 11, 0).await;
    drain(&channel_b, 12, 1).await;
    drop(client_b);
    observed(&channel_b, 12, 0).await;
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();
}

struct Harness {
    _directory: tempfile::TempDir,
    listener: AsyncListener,
    worker: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Harness {
    async fn start(generation: u64, service: Service) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("handoff.sock");
        let listener = AsyncListener::new(HandoffListener::bind(&path).unwrap()).unwrap();
        let worker = tokio::spawn(run_with(GenerationId::from_raw(generation), path, service));
        Self {
            _directory: directory,
            listener,
            worker,
        }
    }

    async fn accept_worker(&self, generation: u64, active: u64) -> Arc<AsyncChannel> {
        accept_generation(&self.listener, generation, active).await
    }
}

async fn accept_generation(
    listener: &AsyncListener,
    generation: u64,
    active: u64,
) -> Arc<AsyncChannel> {
    loop {
        let channel = listener.accept().await.unwrap();
        let Received::Control(Control::WorkerHello {
            generation: found, ..
        }) = channel.receive().await.unwrap()
        else {
            continue;
        };
        if found.raw() != generation {
            continue;
        }
        channel
            .send_control(&Control::CoordinatorHello { epoch: 1 })
            .await
            .unwrap();
        assert!(
            matches!(channel.receive().await.unwrap(), Received::Control(Control::Ready { generation: found }) if found.raw() == generation)
        );
        observed(&channel, generation, active).await;
        return channel;
    }
}

async fn activate(channel: &AsyncChannel, generation: u64) {
    channel
        .send_control(&Control::Activate {
            generation: WireGenerationId::new(generation),
        })
        .await
        .unwrap();
    assert!(
        matches!(channel.receive().await.unwrap(), Received::Control(Control::Accepting { generation: found }) if found.raw() == generation)
    );
}

async fn drain(channel: &AsyncChannel, generation: u64, active: u64) {
    channel
        .send_control(&Control::Drain {
            generation: WireGenerationId::new(generation),
        })
        .await
        .unwrap();
    assert!(
        matches!(channel.receive().await.unwrap(), Received::Control(Control::AdmissionsPaused { generation: found }) if found.raw() == generation)
    );
    observed(channel, generation, active).await;
}

async fn observed(channel: &AsyncChannel, generation: u64, active: u64) {
    assert!(
        matches!(channel.receive().await.unwrap(), Received::Control(Control::ConnectionsObserved { generation: found, active: count }) if found.raw() == generation && count == active)
    );
}

async fn handoff(
    channel: &AsyncChannel,
    id: HandoffId,
    stream: &tokio::net::TcpStream,
    duplicate: bool,
) {
    prepare_handoff(channel, id, stream, duplicate).await;
    commit_handoff(channel, id).await;
}

async fn prepare_handoff(
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
    assert!(
        matches!(channel.receive().await.unwrap(), Received::Control(Control::ConnectionAck { handoff_id }) if handoff_id == id)
    );
}

async fn commit_handoff(channel: &AsyncChannel, id: HandoffId) {
    channel
        .send_control(&Control::ConnectionCommitted { handoff_id: id })
        .await
        .unwrap();
    assert!(
        matches!(channel.receive().await.unwrap(), Received::Control(Control::ConnectionCommitAck { handoff_id }) if handoff_id == id)
    );
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

fn echo_service() -> Service {
    Arc::new(|mut stream, lifetime| {
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

fn counted_service(calls: Arc<AtomicUsize>) -> Service {
    Arc::new(move |mut stream, lifetime| {
        calls.fetch_add(1, Ordering::AcqRel);
        async move {
            let _lifetime = lifetime;
            let mut byte = [0];
            let _ = stream.read(&mut byte).await;
        }
        .boxed()
    })
}

fn counted_echo_service(calls: Arc<AtomicUsize>) -> Service {
    let echo = echo_service();
    Arc::new(move |stream, lifetime| {
        calls.fetch_add(1, Ordering::AcqRel);
        echo(stream, lifetime)
    })
}

fn activation_values<'a>(pid: &'a str, fds: &'a str, name: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("LISTEN_PID", pid),
        ("LISTEN_FDS", fds),
        ("LISTEN_FDNAMES", name),
    ]
}

fn active_deployment(build: &str) -> (DeploymentState, GenerationId) {
    let mut state = DeploymentState::default();
    let DeployPlan::StageTarget { target, .. } =
        state.plan_deploy(BuildId::new(build).unwrap()).unwrap()
    else {
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

fn staged_generation(state: &DeploymentState) -> GenerationId {
    state
        .snapshot()
        .generations
        .into_iter()
        .find_map(|generation| {
            (generation.status == GenerationStatus::Staged).then_some(generation.id)
        })
        .unwrap()
}

fn activation_phase(state: &DeploymentState) -> ActivationPhase {
    state.snapshot().activation.unwrap().phase
}

fn test_paths(root: &std::path::Path, executable: std::path::PathBuf) -> HostPaths {
    HostPaths {
        executable,
        generations: root.join("generations"),
        control: root.join("control.sock"),
        state: root.join("state.json"),
    }
}

async fn test_listener() -> tokio::net::TcpListener {
    tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap()
}
