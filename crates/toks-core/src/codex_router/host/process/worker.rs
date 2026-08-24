use anyhow::Result;
use futures_util::{future::BoxFuture, FutureExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::codex_router::handoff::{HandoffChannel, PeerIdentity, WorkerInstanceId};
use crate::codex_router::proxy::{ConnectionLifetime, ConnectionService, RouterRuntimeHandle};

use super::channel::AsyncChannel;
use super::paths::HostPaths;
use crate::codex_router::host::GenerationId;

mod connection;
mod session;
#[cfg(test)]
mod session_tests;
use connection::Handoffs;
use session::SessionEnd;

pub(super) type Service =
    Arc<dyn Fn(tokio::net::TcpStream, ConnectionLifetime) -> BoxFuture<'static, ()> + Send + Sync>;
pub(super) type PeerAuthorizer =
    Arc<dyn Fn(PeerIdentity) -> BoxFuture<'static, bool> + Send + Sync>;

pub(crate) async fn run(generation: GenerationId) -> Result<()> {
    let instance = worker_instance_id()?;
    let runtime = RouterRuntimeHandle::discover_for_worker(generation.get(), instance.raw())?;
    let connection_service = ConnectionService::new(&runtime);
    let service: Service = Arc::new(move |stream, lifetime| {
        let connection_service = connection_service.clone();
        async move {
            if let Err(error) = connection_service.serve(stream, lifetime).await {
                eprintln!("router worker connection failed: {error:#}");
            }
        }
        .boxed()
    });
    let paths = HostPaths::discover()?;
    let artifact_root = paths
        .generations
        .parent()
        .ok_or_else(|| anyhow::anyhow!("generation directory has no artifact root"))?
        .to_owned();
    let authorizer: PeerAuthorizer = Arc::new(move |peer| {
        super::coordinator_identity::authorize(peer, artifact_root.clone()).boxed()
    });
    run_with_instance(generation, instance, paths.control, service, authorizer).await
}

struct WorkerState {
    active: AtomicBool,
    draining: AtomicBool,
    connections: AtomicU64,
    handoffs: Mutex<Handoffs>,
    count_changed: mpsc::Sender<()>,
    coordinator_epoch: Mutex<Option<u64>>,
}

impl WorkerState {
    fn observed(&self) -> u64 {
        self.connections.load(Ordering::Acquire)
    }
}

#[cfg(test)]
pub(super) async fn run_with(
    generation: GenerationId,
    control_path: std::path::PathBuf,
    service: Service,
) -> Result<()> {
    let authorizer: PeerAuthorizer = Arc::new(|peer| {
        futures_util::future::ready(peer.uid == nix::unistd::Uid::current().as_raw()).boxed()
    });
    run_with_authorizer(generation, control_path, service, authorizer).await
}

#[cfg(test)]
pub(super) async fn run_with_authorizer(
    generation: GenerationId,
    control_path: std::path::PathBuf,
    service: Service,
    authorizer: PeerAuthorizer,
) -> Result<()> {
    run_with_instance(
        generation,
        worker_instance_id()?,
        control_path,
        service,
        authorizer,
    )
    .await
}

async fn run_with_instance(
    generation: GenerationId,
    instance: WorkerInstanceId,
    control_path: std::path::PathBuf,
    service: Service,
    authorizer: PeerAuthorizer,
) -> Result<()> {
    // One wakeup is enough: the session samples the authoritative atomic count
    // after receiving it, so additional close events can coalesce safely.
    let (count_changed, mut count_events) = mpsc::channel(1);
    let state = Arc::new(WorkerState {
        active: AtomicBool::new(false),
        draining: AtomicBool::new(false),
        connections: AtomicU64::new(0),
        handoffs: Mutex::new(Handoffs::default()),
        count_changed,
        coordinator_epoch: Mutex::new(None),
    });
    loop {
        let channel = match HandoffChannel::connect(&control_path) {
            Ok(channel) if authorizer(channel.peer_identity()).await => {
                Arc::new(AsyncChannel::new(channel)?)
            }
            Ok(_) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        match session::run(
            channel,
            generation,
            instance,
            state.clone(),
            service.clone(),
            &mut count_events,
        )
        .await
        {
            SessionEnd::Drained => return Ok(()),
            SessionEnd::Disconnected => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

fn worker_instance_id() -> Result<WorkerInstanceId> {
    loop {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes)?;
        if let Some(instance) = WorkerInstanceId::new(u64::from_ne_bytes(bytes)) {
            return Ok(instance);
        }
    }
}
