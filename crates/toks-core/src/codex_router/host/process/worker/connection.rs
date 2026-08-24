use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::os::fd::OwnedFd;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::codex_router::handoff::{Control, HandoffId};
use crate::codex_router::proxy::ConnectionLifetime;

use super::{AsyncChannel, Service, WorkerState};

#[derive(Default)]
pub(super) struct Handoffs {
    pending: HashMap<HandoffId, tokio::net::TcpStream>,
    // A lost commit acknowledgement can make the coordinator replay this descriptor.
    committed: HashSet<HandoffId>,
}

impl Handoffs {
    pub(super) fn pending_is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn clear_pending(&mut self) {
        self.pending.clear();
    }

    pub(super) fn reconcile_epoch(
        &mut self,
        previous_epoch: Option<u64>,
        epoch: u64,
    ) -> Vec<tokio::net::TcpStream> {
        if previous_epoch.is_none() || previous_epoch == Some(epoch) {
            return Vec::new();
        }
        self.committed.clear();
        self.pending.drain().map(|(_, stream)| stream).collect()
    }

    pub(super) fn finalize(&mut self, handoff_id: HandoffId) {
        self.committed.remove(&handoff_id);
        // The coordinator also finalizes handoffs it abandoned mid-delivery,
        // and those can still be parked pre-commit. That descriptor will never
        // be committed, so drop it instead of holding it for the epoch.
        self.pending.remove(&handoff_id);
    }
}

pub(super) fn adopt_orphans(
    previous_epoch: Option<u64>,
    epoch: u64,
    state: &Arc<WorkerState>,
    service: &Service,
) {
    let streams = state
        .handoffs
        .lock()
        .expect("worker handoff map poisoned")
        .reconcile_epoch(previous_epoch, epoch);
    for stream in streams {
        start(stream, state, service);
    }
}

pub(super) async fn register(
    handoff_id: HandoffId,
    fd: OwnedFd,
    channel: Arc<AsyncChannel>,
    state: &Arc<WorkerState>,
) {
    let stream = std::net::TcpStream::from(fd);
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let Ok(stream) = tokio::net::TcpStream::from_std(stream) else {
        return;
    };
    let acknowledgement = {
        let mut handoffs = state.handoffs.lock().expect("worker handoff map poisoned");
        if handoffs.committed.contains(&handoff_id) {
            Control::ConnectionCommitAck { handoff_id }
        } else {
            if let Entry::Vacant(entry) = handoffs.pending.entry(handoff_id) {
                entry.insert(stream);
            }
            Control::ConnectionAck { handoff_id }
        }
    };
    let _ = channel.send_control(&acknowledgement).await;
}

pub(super) async fn commit(
    handoff_id: HandoffId,
    channel: Arc<AsyncChannel>,
    state: &Arc<WorkerState>,
    service: &Service,
) {
    let commit = {
        let mut handoffs = state.handoffs.lock().expect("worker handoff map poisoned");
        if handoffs.committed.contains(&handoff_id) {
            Commit::AlreadyCommitted
        } else {
            let stream = handoffs.pending.remove(&handoff_id);
            if stream.is_some() {
                handoffs.committed.insert(handoff_id);
            }
            match stream {
                Some(stream) => Commit::New(stream),
                None => Commit::Unknown,
            }
        }
    };
    match commit {
        Commit::New(stream) => start(stream, state, service),
        Commit::AlreadyCommitted => {}
        Commit::Unknown => return,
    }
    let _ = channel
        .send_control(&Control::ConnectionCommitAck { handoff_id })
        .await;
}

fn start(stream: tokio::net::TcpStream, state: &Arc<WorkerState>, service: &Service) {
    state.connections.fetch_add(1, Ordering::AcqRel);
    let state_for_task = state.clone();
    let lifetime = ConnectionLifetime::new(move || {
        state_for_task.connections.fetch_sub(1, Ordering::AcqRel);
        let _ = state_for_task.count_changed.try_send(());
    });
    tokio::spawn(service(stream, lifetime));
}

enum Commit {
    New(tokio::net::TcpStream),
    AlreadyCommitted,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::Handoffs;
    use crate::codex_router::handoff::HandoffId;

    #[test]
    fn finalized_commit_reclaims_its_idempotency_tombstone() {
        let id = HandoffId::new(17, 3);
        let mut handoffs = Handoffs::default();
        handoffs.committed.insert(id);

        handoffs.finalize(id);

        assert!(!handoffs.committed.contains(&id));
    }

    #[tokio::test]
    async fn finalizing_an_abandoned_handoff_drops_its_parked_descriptor() {
        let id = HandoffId::new(17, 5);
        let mut handoffs = Handoffs::default();
        handoffs.pending.insert(id, stream().await);

        handoffs.finalize(id);

        assert!(handoffs.pending.is_empty());
    }

    async fn stream() -> tokio::net::TcpStream {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = tokio::spawn(tokio::net::TcpStream::connect(address));
        listener.accept().await.unwrap();
        client.await.unwrap().unwrap()
    }

    #[test]
    fn coordinator_epoch_change_reclaims_old_commit_tombstones() {
        let id = HandoffId::new(17, 4);
        let mut handoffs = Handoffs::default();
        handoffs.committed.insert(id);

        assert!(handoffs.reconcile_epoch(Some(17), 18).is_empty());

        assert!(handoffs.committed.is_empty());
    }
}
