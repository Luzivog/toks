use anyhow::Result;
use futures_util::future::BoxFuture;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::codex_router::handoff::{Connection, HandoffId, WorkerInstanceId};
use crate::codex_router::host::{BuildId, DeploymentState, GenerationId, GenerationStatus};

use self::workers::Workers;
use super::pending::Pending;
use super::wait::DeploymentWait;
use crate::codex_router::host::process::channel::AsyncChannel;
use crate::codex_router::host::process::paths::HostPaths;

mod admission;
mod workers;

pub(super) const CONTROL_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

pub(super) type WorkerCommand =
    Arc<dyn Fn(&'static str, Vec<GenerationId>) -> BoxFuture<'static, Result<()>> + Send + Sync>;
pub(super) type WorkerInventory = Arc<
    dyn Fn() -> BoxFuture<'static, Result<BTreeMap<GenerationId, super::worker_unit::Liveness>>>
        + Send
        + Sync,
>;

pub(in crate::codex_router::host::process) struct WorkerSlot {
    pub(in crate::codex_router::host::process) registration: u64,
    pub(in crate::codex_router::host::process) instance: WorkerInstanceId,
    pub(in crate::codex_router::host::process) ready: bool,
    pub(in crate::codex_router::host::process) accepting: bool,
    pub(in crate::codex_router::host::process) draining: bool,
    pub(in crate::codex_router::host::process) pending_reconciled: bool,
    pub(in crate::codex_router::host::process) channel: Arc<AsyncChannel>,
}

pub(in crate::codex_router::host::process) struct Coordinator {
    pub(in crate::codex_router::host::process) listener: tokio::net::TcpListener,
    pub(in crate::codex_router::host::process) paths: HostPaths,
    pub(in crate::codex_router::host::process) deployment: DeploymentState,
    pub(in crate::codex_router::host::process) build: BuildId,
    pub(in crate::codex_router::host::process) pending: Pending,
    pub(in crate::codex_router::host::process) workers: Workers,
    pub(in crate::codex_router::host::process) active: Option<GenerationId>,
    pub(in crate::codex_router::host::process) worker_command: WorkerCommand,
    pub(in crate::codex_router::host::process) worker_inventory: WorkerInventory,
    pub(in crate::codex_router::host::process) deployment_wait: DeploymentWait,
    pub(in crate::codex_router::host::process) consumed_retry_intent:
        Option<crate::codex_router::host::RetryIntent>,
    pub(in crate::codex_router::host::process) retry_cursor: usize,
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
            workers: Workers::new(disconnected_workers),
            active,
            worker_command: default_worker_command(),
            #[cfg(not(test))]
            worker_inventory: Arc::new(|| Box::pin(super::worker_unit::inventory())),
            #[cfg(test)]
            worker_inventory: Arc::new(|| Box::pin(async { Ok(BTreeMap::new()) })),
            deployment_wait: DeploymentWait::default(),
            consumed_retry_intent,
            retry_cursor: 0,
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
                let instance = self.workers.ready_instance(generation.id)?;
                Some((generation.id.get(), instance))
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
            .channel_for(generation)
            .ok_or_else(|| anyhow::anyhow!("worker disconnected"))?;
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

#[cfg(not(test))]
fn default_worker_command() -> WorkerCommand {
    Arc::new(|action, generations| Box::pin(super::worker_unit::run(action, generations)))
}

#[cfg(test)]
fn default_worker_command() -> WorkerCommand {
    Arc::new(|_, _| Box::pin(async { Ok(()) }))
}
