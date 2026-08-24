use futures_util::FutureExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::channel::{AsyncChannel, AsyncListener};
use super::test_fixtures::tcp_pair;
use super::worker::{run_with, Service};
use crate::codex_router::handoff::{
    Connection, Control, GenerationId as WireGenerationId, HandoffId, HandoffListener, Received,
};
use crate::codex_router::host::GenerationId;

#[tokio::test]
async fn new_coordinator_epoch_adopts_prepared_handoff_exactly_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.sock");
    let listener = AsyncListener::new(HandoffListener::bind(&path).unwrap()).unwrap();
    let generation = GenerationId::from_raw(31);
    let worker = tokio::spawn(run_with(generation, path, echo_service(calls.clone())));
    let (first, instance) = accept_epoch(&listener, generation, 401, 0).await;
    activate(&first, generation).await;
    let (mut client, server) = tcp_pair().await;
    let id = HandoffId::new(401, 1);
    first
        .send_connection(
            Connection {
                handoff_id: id,
                duplicate: false,
            },
            &server,
        )
        .await
        .unwrap();
    assert!(matches!(
        receive(&first).await,
        Received::Control(Control::ConnectionAck { handoff_id }) if handoff_id == id
    ));
    drop(server);
    drop(first);

    let (replacement, replacement_instance) = accept_epoch(&listener, generation, 402, 1).await;
    assert_eq!(replacement_instance, instance);
    activate(&replacement, generation).await;
    round_trip(&mut client, b"adopted").await;
    assert_eq!(calls.load(Ordering::Acquire), 1);

    drop(replacement);
    let (same_epoch, same_epoch_instance) = accept_epoch(&listener, generation, 402, 1).await;
    assert_eq!(same_epoch_instance, instance);
    round_trip(&mut client, b"still-once").await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    drain(&same_epoch, generation, 1).await;
    drop(client);
    assert!(matches!(
        receive(&same_epoch).await,
        Received::Control(Control::ConnectionsObserved { active: 0, .. })
    ));
    worker.await.unwrap().unwrap();
}

async fn accept_epoch(
    listener: &AsyncListener,
    generation: GenerationId,
    epoch: u64,
    active: u64,
) -> (
    Arc<AsyncChannel>,
    crate::codex_router::handoff::WorkerInstanceId,
) {
    let channel = listener.accept().await.unwrap();
    let Received::Control(Control::WorkerHello {
        generation: found,
        instance,
    }) = receive(&channel).await
    else {
        panic!("expected worker hello")
    };
    assert_eq!(found.raw(), generation.get());
    channel
        .send_control(&Control::CoordinatorHello { epoch })
        .await
        .unwrap();
    assert!(matches!(
        receive(&channel).await,
        Received::Control(Control::Ready { generation: found })
            if found.raw() == generation.get()
    ));
    assert!(matches!(
        receive(&channel).await,
        Received::Control(Control::ConnectionsObserved { active: found, .. }) if found == active
    ));
    (channel, instance)
}

async fn activate(channel: &AsyncChannel, generation: GenerationId) {
    channel
        .send_control(&Control::Activate {
            generation: WireGenerationId::new(generation.get()),
        })
        .await
        .unwrap();
    assert!(matches!(
        receive(channel).await,
        Received::Control(Control::Accepting { generation: found })
            if found.raw() == generation.get()
    ));
}

async fn drain(channel: &AsyncChannel, generation: GenerationId, active: u64) {
    channel
        .send_control(&Control::Drain {
            generation: WireGenerationId::new(generation.get()),
        })
        .await
        .unwrap();
    assert!(matches!(
        receive(channel).await,
        Received::Control(Control::AdmissionsPaused { .. })
    ));
    assert!(matches!(
        receive(channel).await,
        Received::Control(Control::ConnectionsObserved { active: found, .. }) if found == active
    ));
}

async fn receive(channel: &AsyncChannel) -> Received {
    tokio::time::timeout(Duration::from_secs(2), channel.receive())
        .await
        .expect("timed out waiting for worker")
        .unwrap()
}

async fn round_trip(stream: &mut tokio::net::TcpStream, message: &[u8]) {
    stream.write_all(message).await.unwrap();
    let mut found = vec![0; message.len()];
    stream.read_exact(&mut found).await.unwrap();
    assert_eq!(found, message);
}

fn echo_service(calls: Arc<AtomicUsize>) -> Service {
    Arc::new(move |mut stream, lifetime| {
        calls.fetch_add(1, Ordering::AcqRel);
        async move {
            let _lifetime = lifetime;
            let mut bytes = [0_u8; 128];
            while let Ok(size) = stream.read(&mut bytes).await {
                if size == 0 || stream.write_all(&bytes[..size]).await.is_err() {
                    break;
                }
            }
        }
        .boxed()
    })
}
