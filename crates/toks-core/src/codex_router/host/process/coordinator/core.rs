use anyhow::Result;
use futures_util::future::BoxFuture;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::codex_router::handoff::{Connection, HandoffId, WorkerInstanceId};
use crate::codex_router::host::{BuildId, DeploymentState, GenerationId, GenerationStatus};

use super::super::channel::AsyncChannel;
use super::super::paths::HostPaths;
use super::pending::Pending;
use super::wait::DeploymentWait;

mod admission;

pub(super) const CONTROL_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

pub(super) type WorkerCommand =
    Arc<dyn Fn(&'static str, Vec<GenerationId>) -> BoxFuture<'static, Result<()>> + Send + Sync>;
pub(super) type WorkerInventory = Arc<
    dyn Fn() -> BoxFuture<'static, Result<BTreeMap<GenerationId, super::worker_unit::Liveness>>>
        + Send
        + Sync,
>;

pub(super) struct WorkerSlot {
    pub(super) registration: u64,
    pub(super) instance: WorkerInstanceId,
    pub(super) ready: bool,
    pub(super) accepting: bool,
    pub(super) draining: bool,
    pub(super) pending_reconciled: bool,
    pub(super) channel: Arc<AsyncChannel>,
}

pub(in crate::codex_router::host::process) struct Coordinator {
    pub(super) listener: tokio::net::TcpListener,
    pub(in crate::codex_router::host::process) paths: HostPaths,
    pub(in crate::codex_router::host::process) deployment: DeploymentState,
    pub(super) build: BuildId,
    pub(super) pending: Pending,
    pub(super) workers: HashMap<GenerationId, WorkerSlot>,
    pub(in crate::codex_router::host::process) active: Option<GenerationId>,
    pub(super) worker_command: WorkerCommand,
    pub(super) worker_inventory: WorkerInventory,
    pub(super) deployment_wait: DeploymentWait,
    pub(super) stopped_workers: BTreeSet<GenerationId>,
    pub(super) disconnected_workers: BTreeSet<GenerationId>,
    pub(super) consumed_retry_intent: Option<crate::codex_router::host::RetryIntent>,
    pub(super) retry_cursor: usize,
    pub(super) next_registration: u64,
}

impl Coordinator {
    pub(in crate::codex_router::host::process) fn new(
        listener: tokio::net::TcpListener,
        paths: HostPaths,
        mut deployment: DeploymentState,
    ) -> Result<Self> {
        deployment.reserve_generation_ids_after(paths.highest_generation()?)?;
        let build = paths.build_id()?;
        let retry = crate::codex_router::host::load_retry_intent(&paths.state)?;
        let active = deployment
            .snapshot()
            .generations
            .iter()
            .find_map(|generation| {
                (generation.status == GenerationStatus::Active).then_some(generation.id)
            });
        let terminal = matches!(
            deployment.current_plan()?,
            crate::codex_router::host::DeployPlan::Settled { .. }
                | crate::codex_router::host::DeployPlan::Unavailable { .. }
        );
        let consumed_retry_intent = if terminal {
            match retry.filter(|intent| intent.build == build) {
                Some(intent) if deployment.consume_retry(build.clone(), intent.id.clone())? => {
                    Some(intent)
                }
                _ => None,
            }
        } else {
            None
        };
        if terminal && consumed_retry_intent.is_none() {
            deployment.plan_deploy(build.clone())?;
        }
        let disconnected_workers = deployment
            .snapshot()
            .generations
            .into_iter()
            .filter(|generation| {
                !matches!(
                    generation.status,
                    GenerationStatus::Failed | GenerationStatus::Retired
                )
            })
            .map(|generation| generation.id)
            .collect();
        Ok(Self {
            listener,
            paths,
            deployment,
            build,
            pending: Pending::new()?,
            workers: HashMap::new(),
            active,
            worker_command: Arc::new(|action, generations| {
                Box::pin(super::worker_unit::run(action, generations))
            }),
            #[cfg(not(test))]
            worker_inventory: Arc::new(|| Box::pin(super::worker_unit::inventory())),
            #[cfg(test)]
            worker_inventory: Arc::new(|| Box::pin(async { Ok(BTreeMap::new()) })),
            deployment_wait: DeploymentWait::default(),
            stopped_workers: BTreeSet::new(),
            disconnected_workers,
            consumed_retry_intent,
            retry_cursor: 0,
            next_registration: 1,
        })
    }

    pub(super) fn epoch(&self) -> u64 {
        self.pending.epoch()
    }

    pub(crate) fn reconcilable_worker_instances(&self) -> Option<BTreeMap<u64, u64>> {
        self.deployment
            .snapshot()
            .generations
            .into_iter()
            .filter(|generation| {
                matches!(
                    generation.status,
                    crate::codex_router::host::GenerationStatus::Staged
                        | crate::codex_router::host::GenerationStatus::Active
                        | crate::codex_router::host::GenerationStatus::Draining
                )
            })
            .map(|generation| {
                let worker = self.workers.get(&generation.id)?;
                worker
                    .ready
                    .then_some((generation.id.get(), worker.instance.raw()))
            })
            .collect()
    }

    pub(super) async fn handoff(&mut self, stream: tokio::net::TcpStream) -> Result<()> {
        let generation = self
            .active
            .ok_or_else(|| anyhow::anyhow!("no active worker"))?;
        let id = self.pending.insert(generation.into(), stream)?;
        self.send_pending(generation, id).await
    }

    pub(super) async fn send_pending(
        &mut self,
        generation: GenerationId,
        id: HandoffId,
    ) -> Result<()> {
        let channel = self
            .workers
            .get(&generation)
            .ok_or_else(|| anyhow::anyhow!("worker disconnected"))?
            .channel
            .clone();
        let wire_generation = generation.into();
        let (stream, duplicate) = self
            .pending
            .delivery(wire_generation, id)
            .ok_or_else(|| anyhow::anyhow!("unknown pending handoff"))?;
        tokio::time::timeout(
            CONTROL_SEND_TIMEOUT,
            channel.send_connection(
                Connection {
                    handoff_id: id,
                    duplicate,
                },
                stream,
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("worker handoff send timed out"))??;
        self.pending.mark_preparing(wire_generation, id);
        Ok(())
    }
}
