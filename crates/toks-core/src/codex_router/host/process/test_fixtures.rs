use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::channel::AsyncChannel;
use super::coordinator::{Coordinator, WorkerSlot};
use super::paths::HostPaths;
use crate::codex_router::handoff::{HandoffChannel, HandoffListener, WorkerInstanceId};
use crate::codex_router::host::{
    BuildId, DeployPlan, DeploymentEvent, DeploymentState, GenerationId,
};

pub(super) async fn fixture() -> (
    tempfile::TempDir,
    Coordinator,
    Arc<AsyncChannel>,
    GenerationId,
) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("router");
    std::fs::write(&executable, b"same-build").unwrap();
    let paths = host_paths(directory.path(), executable);
    let (deployment, generation) = active_deployment(paths.build_id().unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut coordinator = Coordinator::new(listener, paths, deployment).unwrap();
    let (channel, peer) = channel_pair();
    coordinator.active = Some(generation);
    coordinator.workers.replace(
        generation,
        accepting_worker(
            channel,
            1,
            WorkerInstanceId::new(1).expect("fixture worker instance must be nonzero"),
        ),
    );
    (directory, coordinator, peer, generation)
}

pub(super) fn active_deployment(build: BuildId) -> (DeploymentState, GenerationId) {
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

pub(super) fn channel_pair() -> (Arc<AsyncChannel>, Arc<AsyncChannel>) {
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

pub(super) async fn tcp_pair() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = tokio::spawn(tokio::net::TcpStream::connect(address));
    let (server, _) = listener.accept().await.unwrap();
    (client.await.unwrap().unwrap(), server)
}

pub(super) fn host_paths(root: &Path, executable: PathBuf) -> HostPaths {
    HostPaths {
        executable,
        generations: root.join("generations"),
        control: root.join("control.sock"),
        state: root.join("state.json"),
    }
}

pub(super) fn connected_worker(
    channel: Arc<AsyncChannel>,
    registration: u64,
    instance: WorkerInstanceId,
) -> WorkerSlot {
    worker_slot(channel, registration, instance, false, false, false)
}

pub(super) fn ready_worker(
    channel: Arc<AsyncChannel>,
    registration: u64,
    instance: WorkerInstanceId,
) -> WorkerSlot {
    worker_slot(channel, registration, instance, true, false, true)
}

pub(super) fn accepting_worker(
    channel: Arc<AsyncChannel>,
    registration: u64,
    instance: WorkerInstanceId,
) -> WorkerSlot {
    worker_slot(channel, registration, instance, true, true, true)
}

fn worker_slot(
    channel: Arc<AsyncChannel>,
    registration: u64,
    instance: WorkerInstanceId,
    ready: bool,
    accepting: bool,
    pending_reconciled: bool,
) -> WorkerSlot {
    WorkerSlot {
        registration,
        instance,
        ready,
        accepting,
        draining: false,
        pending_reconciled,
        channel,
    }
}
