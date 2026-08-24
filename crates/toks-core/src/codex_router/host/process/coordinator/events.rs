use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::codex_router::handoff::{Control, Received, WorkerInstanceId};
use crate::codex_router::host::GenerationId;

use super::super::channel::AsyncChannel;

pub(super) enum WorkerEvent {
    Connected {
        generation: GenerationId,
        instance: WorkerInstanceId,
        pid: i32,
        channel: Arc<AsyncChannel>,
    },
    Message {
        generation: GenerationId,
        registration: u64,
        message: Control,
    },
    Disconnected {
        generation: GenerationId,
        registration: u64,
    },
}

pub(super) fn spawn_reader(
    generation: GenerationId,
    registration: u64,
    channel: Arc<AsyncChannel>,
    events: UnboundedSender<WorkerEvent>,
) {
    tokio::spawn(async move {
        while let Ok(Received::Control(message)) = channel.receive().await {
            let _ = events.send(WorkerEvent::Message {
                generation,
                registration,
                message,
            });
        }
        let _ = events.send(WorkerEvent::Disconnected {
            generation,
            registration,
        });
    });
}
