use futures_util::FutureExt;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use super::super::channel::AsyncChannel;
use super::super::test_fixtures::channel_pair;
use super::{session, Handoffs, Service, WorkerState};
use crate::codex_router::handoff::{
    Control, GenerationId as WireGenerationId, Received, WorkerInstanceId,
};
use crate::codex_router::host::GenerationId;

#[tokio::test(flavor = "current_thread")]
async fn delayed_old_zero_cannot_retire_a_newer_connection() {
    let (coordinator, worker_channel) = channel_pair();
    let (count_changed, mut count_events) = tokio::sync::mpsc::channel(1);
    let state = Arc::new(WorkerState {
        active: false.into(),
        draining: false.into(),
        connections: 0.into(),
        handoffs: Mutex::new(Handoffs::default()),
        count_changed,
        coordinator_epoch: Mutex::new(None),
    });
    let generation = GenerationId::from_raw(41);
    let wire_generation = generation.into();
    let instance = WorkerInstanceId::new(91).unwrap();
    let service: Service = Arc::new(|_, _| async {}.boxed());
    let state_for_session = state.clone();
    let worker = tokio::spawn(async move {
        session::run(
            worker_channel,
            generation,
            instance,
            state_for_session,
            service,
            &mut count_events,
        )
        .await
    });

    assert!(matches!(
        receive(&coordinator).await,
        Received::Control(Control::WorkerHello { generation: found, .. })
            if found == wire_generation
    ));
    coordinator
        .send_control(&Control::CoordinatorHello { epoch: 1 })
        .await
        .unwrap();
    assert!(matches!(
        receive(&coordinator).await,
        Received::Control(Control::Ready { generation: found }) if found == wire_generation
    ));
    assert_observed(&coordinator, wire_generation, 0).await;
    coordinator
        .send_control(&Control::Activate {
            generation: wire_generation,
        })
        .await
        .unwrap();
    assert!(matches!(
        receive(&coordinator).await,
        Received::Control(Control::Accepting { generation: found }) if found == wire_generation
    ));

    // A zero captured by an older close is delivered only after a newer
    // connection has opened and admissions have paused.
    state.connections.store(1, Ordering::Release);
    coordinator
        .send_control(&Control::Drain {
            generation: wire_generation,
        })
        .await
        .unwrap();
    assert!(matches!(
        receive(&coordinator).await,
        Received::Control(Control::AdmissionsPaused { generation: found })
            if found == wire_generation
    ));
    assert_observed(&coordinator, wire_generation, 1).await;
    state.count_changed.try_send(()).unwrap();
    assert_observed(&coordinator, wire_generation, 1).await;

    state.connections.store(0, Ordering::Release);
    state.count_changed.try_send(()).unwrap();
    assert_observed(&coordinator, wire_generation, 0).await;
    assert!(matches!(
        worker.await.unwrap(),
        session::SessionEnd::Drained
    ));
}

async fn assert_observed(channel: &AsyncChannel, generation: WireGenerationId, active: u64) {
    assert!(matches!(
        receive(channel).await,
        Received::Control(Control::ConnectionsObserved {
            generation: found,
            active: found_active,
        }) if found == generation && found_active == active
    ));
}

async fn receive(channel: &AsyncChannel) -> Received {
    tokio::time::timeout(std::time::Duration::from_secs(2), channel.receive())
        .await
        .expect("timed out waiting for worker control message")
        .unwrap()
}
