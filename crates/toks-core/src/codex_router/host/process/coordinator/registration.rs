use anyhow::Result;

use crate::codex_router::handoff::Control;
use crate::codex_router::host::GenerationId;

use super::core::{Coordinator, WorkerSlot};
use super::events::{spawn_reader, WorkerEvent};

impl Coordinator {
    pub(super) async fn worker_event(
        &mut self,
        event: WorkerEvent,
        events: tokio::sync::mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<()> {
        match event {
            WorkerEvent::Connected {
                generation,
                instance,
                pid,
                channel,
            } => {
                if !self.known_generation(generation)
                    || !self
                        .paths
                        .worker_matches(GenerationId::from_raw(generation), pid)
                {
                    return Ok(());
                }
                let registration = self.next_registration;
                self.next_registration = self.next_registration.saturating_add(1);
                self.disconnected_workers
                    .remove(&GenerationId::from_raw(generation));
                self.workers.insert(
                    generation,
                    WorkerSlot {
                        registration,
                        instance,
                        ready: false,
                        accepting: false,
                        draining: false,
                        pending_reconciled: false,
                        channel: channel.clone(),
                    },
                );
                spawn_reader(generation, registration, channel, events);
            }
            WorkerEvent::Message {
                generation,
                registration,
                message,
            } if self.current(generation, registration) || is_connection_ack(&message) => {
                self.handle_message(generation, message).await?;
            }
            WorkerEvent::Disconnected {
                generation,
                registration,
            } if self.current(generation, registration) => {
                self.workers.remove(&generation);
                self.deployment_wait
                    .clear_generation(GenerationId::from_raw(generation));
                self.worker_disconnected(generation)?;
            }
            _ => {}
        }
        self.advance().await?;
        Ok(())
    }

    fn current(&self, generation: u64, registration: u64) -> bool {
        self.workers
            .get(&generation)
            .is_some_and(|worker| worker.registration == registration)
    }

    fn known_generation(&self, generation: u64) -> bool {
        self.deployment.snapshot().generations.iter().any(|found| {
            found.id.get() == generation
                && matches!(
                    found.status,
                    crate::codex_router::host::GenerationStatus::Staged
                        | crate::codex_router::host::GenerationStatus::Active
                        | crate::codex_router::host::GenerationStatus::Draining
                )
        })
    }
}

/// Whether a message is a connection acknowledgement rather than a statement
/// about the worker's own lifecycle.
///
/// These name their handoff by id and drive only idempotent transitions, so
/// they stay valid across a worker reconnect. Dropping one because a newer
/// registration replaced the sender strands that handoff, and the client
/// descriptor behind it, until the settle timeout reclaims them.
fn is_connection_ack(message: &Control) -> bool {
    matches!(
        message,
        Control::ConnectionAck { .. }
            | Control::ConnectionCommitAck { .. }
            | Control::ConnectionFinalizedAck { .. }
    )
}
