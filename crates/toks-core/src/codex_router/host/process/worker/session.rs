use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::codex_router::handoff::{
    Control, GenerationId as WireGenerationId, Received, WorkerInstanceId,
};
use crate::codex_router::host::GenerationId;

use super::super::channel::AsyncChannel;
use super::{connection, Service, WorkerState};

pub(super) enum SessionEnd {
    Drained,
    Disconnected,
}

pub(super) async fn run(
    channel: Arc<AsyncChannel>,
    generation: GenerationId,
    instance: WorkerInstanceId,
    state: Arc<WorkerState>,
    service: Service,
    counts: &mut tokio::sync::mpsc::Receiver<()>,
) -> SessionEnd {
    let wire_generation = WireGenerationId::new(generation.get());
    if channel
        .send_control(&Control::WorkerHello {
            generation: wire_generation,
            instance,
        })
        .await
        .is_err()
    {
        return SessionEnd::Disconnected;
    }
    let Ok(Received::Control(Control::CoordinatorHello { epoch })) = channel.receive().await else {
        return SessionEnd::Disconnected;
    };
    let previous_epoch = state
        .coordinator_epoch
        .lock()
        .expect("worker coordinator epoch poisoned")
        .replace(epoch);
    connection::adopt_orphans(previous_epoch, epoch, &state, &service);
    for control in [
        Control::Ready {
            generation: wire_generation,
        },
        observed(wire_generation, state.observed()),
    ] {
        if channel.send_control(&control).await.is_err() {
            return SessionEnd::Disconnected;
        }
    }
    loop {
        if state.draining.load(Ordering::Acquire)
            && state.observed() == 0
            && state
                .handoffs
                .lock()
                .expect("worker handoff map poisoned")
                .pending_is_empty()
        {
            let _ = channel.send_control(&observed(wire_generation, 0)).await;
            return SessionEnd::Drained;
        }
        tokio::select! {
            received = channel.receive() => match received {
                Ok(message) => handle(message, channel.clone(), wire_generation, &state, &service).await,
                Err(_) => return SessionEnd::Disconnected,
            },
            Some(()) = counts.recv() => {
                if channel
                    .send_control(&observed(wire_generation, state.observed()))
                    .await
                    .is_err()
                {
                    return SessionEnd::Disconnected;
                }
            }
        }
    }
}

fn observed(generation: WireGenerationId, active: u64) -> Control {
    Control::ConnectionsObserved { generation, active }
}

async fn handle(
    received: Received,
    channel: Arc<AsyncChannel>,
    generation: WireGenerationId,
    state: &Arc<WorkerState>,
    service: &Service,
) {
    match received {
        Received::Control(Control::Activate { generation: found }) if found == generation => {
            state.draining.store(false, Ordering::Release);
            state.active.store(true, Ordering::Release);
            let _ = channel
                .send_control(&Control::Accepting { generation })
                .await;
        }
        Received::Control(Control::Drain { generation: found }) if found == generation => {
            state.active.store(false, Ordering::Release);
            state.draining.store(true, Ordering::Release);
            state
                .handoffs
                .lock()
                .expect("worker handoff map poisoned")
                .clear_pending();
            let _ = channel
                .send_control(&Control::AdmissionsPaused { generation })
                .await;
            let _ = channel
                .send_control(&observed(generation, state.observed()))
                .await;
        }
        Received::Connection(connection, fd) if state.active.load(Ordering::Acquire) => {
            connection::register(connection.handoff_id, fd, channel, state).await;
        }
        Received::Control(Control::ConnectionCommitted { handoff_id }) => {
            connection::commit(handoff_id, channel, state, service).await;
        }
        Received::Control(Control::ConnectionFinalized { handoff_id }) => {
            state
                .handoffs
                .lock()
                .expect("worker handoff map poisoned")
                .finalize(handoff_id);
            let _ = channel
                .send_control(&Control::ConnectionFinalizedAck { handoff_id })
                .await;
        }
        Received::Connection(_, _) | Received::Control(_) => {}
    }
}
