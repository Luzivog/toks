use anyhow::{Context, Result};
use std::sync::Arc;

use crate::codex_router::handoff::{HandoffListener, Received};
use crate::codex_router::host::COORDINATOR_SHUTDOWN_DRAIN_TIMEOUT;
use crate::codex_router::proxy::RouterRuntimeHandle;

use super::activated::systemd_listener;
use super::channel::{AsyncChannel, AsyncListener};
use super::coordinator::events::WorkerEvent;
use super::paths::{load_state, save_state, HostPaths};

pub(super) use self::core::Coordinator;

#[cfg(test)]
mod admission_recovery_tests;
mod advance;
mod core;
mod deployment;
mod events;
#[cfg(test)]
mod exact_once_tests;
mod lifecycle;
mod pending;
#[cfg(test)]
mod pending_retry_tests;
mod planning;
#[cfg(test)]
mod planning_tests;
#[cfg(test)]
mod recovery_tests;
mod registration;
#[cfg(test)]
mod restart_integration_tests;
mod retry;
mod unit_reconciliation;
mod wait;
mod worker_unit;
mod workers;

pub(crate) async fn run(runtime: RouterRuntimeHandle) -> Result<()> {
    let paths = HostPaths::discover()?;
    paths.prepare_control_socket()?;
    let listener = systemd_listener()?;
    let control = HandoffListener::bind(&paths.control)?;
    let deployment = load_state(&paths.state)?;
    let mut coordinator = Coordinator::new(listener, paths, deployment)?;
    save_state(&coordinator.paths.state, &coordinator.deployment)?;
    if let Some(intent) = &coordinator.consumed_retry_intent {
        crate::codex_router::host::clear_retry_intent(&coordinator.paths.state, intent)?;
    }
    coordinator.advance().await?;
    let control = AsyncListener::new(control)?;
    let (events, mut worker_events) = tokio::sync::mpsc::unbounded_channel();
    let mut stopping = None;
    let mut terminate = termination_signal()?;
    let mut reconcile = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut reconciled_instances = None;
    let mut admitting = true;
    reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        if shutdown_complete(stopping, &coordinator, tokio::time::Instant::now()) {
            return Ok(());
        }
        let accepts_clients = stopping.is_none() && coordinator.accepts_clients();
        report_admission_change(&coordinator, accepts_clients, &mut admitting);
        tokio::select! {
            accepted = coordinator.listener.accept(), if accepts_clients => {
                let (stream, _) = accepted.context("accepting router client")?;
                if let Err(error) = coordinator.handoff(stream).await {
                    eprintln!("router handoff deferred: {error:#}");
                }
            }
            accepted = control.accept() => {
                if let Ok(channel) = accepted {
                    spawn_handshake(channel, events.clone(), coordinator.epoch());
                }
            }
            Some(event) = worker_events.recv() => {
                coordinator.worker_event(event, events.clone()).await?;
            }
            _ = terminate.recv(), if stopping.is_none() => {
                stopping = Some(tokio::time::Instant::now());
            }
            _ = reconcile.tick() => {
                coordinator.reap_stale_handoffs();
                coordinator.retry_all_pending().await;
                coordinator.expire_waits().await?;
                coordinator.advance().await?;
            }
        }
        reconcile_connection_owners(&coordinator, &runtime, &mut reconciled_instances)?;
    }
}

/// Reports each transition in and out of admitting clients.
///
/// Edge-triggered so a steady state stays quiet, but a coordinator that stops
/// serving always says so once instead of looking like a network hang.
fn report_admission_change(coordinator: &Coordinator, accepts_clients: bool, admitting: &mut bool) {
    if accepts_clients == *admitting {
        return;
    }
    *admitting = accepts_clients;
    match coordinator.admission_block() {
        Some(reason) if !accepts_clients => eprintln!("router stopped admitting clients: {reason}"),
        _ => eprintln!("router is admitting clients"),
    }
}

fn reconcile_connection_owners(
    coordinator: &Coordinator,
    runtime: &RouterRuntimeHandle,
    previous: &mut Option<std::collections::BTreeMap<u64, u64>>,
) -> Result<()> {
    let Some(current) = coordinator.reconcilable_worker_instances() else {
        return Ok(());
    };
    if previous.as_ref() == Some(&current) {
        return Ok(());
    }
    runtime.reconcile_connection_owners(&current)?;
    *previous = Some(current);
    Ok(())
}

fn shutdown_complete(
    stopping: Option<tokio::time::Instant>,
    coordinator: &Coordinator,
    now: tokio::time::Instant,
) -> bool {
    stopping.is_some_and(|started| {
        coordinator.pending.is_empty()
            || now.duration_since(started) >= COORDINATOR_SHUTDOWN_DRAIN_TIMEOUT
    })
}

fn spawn_handshake(
    channel: Arc<AsyncChannel>,
    events: tokio::sync::mpsc::UnboundedSender<WorkerEvent>,
    epoch: u64,
) {
    tokio::spawn(async move {
        if !trusted_peer(
            channel.peer_identity().uid,
            nix::unistd::Uid::current().as_raw(),
        ) {
            return;
        }
        if let Ok(Received::Control(crate::codex_router::handoff::Control::WorkerHello {
            generation,
            instance,
        })) = channel.receive().await
        {
            if !matches!(
                tokio::time::timeout(
                    self::core::CONTROL_SEND_TIMEOUT,
                    channel.send_control(
                        &crate::codex_router::handoff::Control::CoordinatorHello { epoch }
                    ),
                )
                .await,
                Ok(Ok(()))
            ) {
                return;
            }
            let _ = events.send(WorkerEvent::Connected {
                generation: generation.raw(),
                instance,
                pid: channel.peer_identity().pid,
                channel,
            });
        }
    });
}

pub(super) fn trusted_peer(peer_uid: u32, coordinator_uid: u32) -> bool {
    peer_uid == coordinator_uid
}

fn termination_signal() -> Result<tokio::signal::unix::Signal> {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("installing router termination signal")
}
