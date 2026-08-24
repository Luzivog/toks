use super::connection::Handoffs;
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
