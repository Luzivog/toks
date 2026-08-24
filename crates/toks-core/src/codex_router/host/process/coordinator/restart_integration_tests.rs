use futures_util::FutureExt;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::super::channel::{AsyncChannel, AsyncListener};
use super::super::paths::{load_state, save_state, HostPaths};
use super::super::worker::{run_with, Service};
use super::core::{Coordinator, WorkerSlot};
use crate::codex_router::handoff::{
    Control, GenerationId as WireGenerationId, HandoffListener, Received, WorkerInstanceId,
};
use crate::codex_router::host::{DeployPlan, DeploymentEvent, DeploymentState, GenerationId};

const STEP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[tokio::test]
async fn persistent_listener_and_worker_survive_coordinator_replacement() {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        coordinator_replacement_scenario(),
    )
    .await
    .expect("coordinator replacement scenario timed out");
}

async fn coordinator_replacement_scenario() {
    let directory = tempfile::tempdir().unwrap();
    let paths = test_paths(directory.path());
    let (deployment, generation) = active_deployment(&paths);
    let generation_directory = paths.generations.join(generation.get().to_string());
    std::fs::create_dir_all(&generation_directory).unwrap();
    std::os::unix::fs::symlink(&paths.executable, generation_directory.join("toks-router"))
        .unwrap();
    save_state(&paths.state, &deployment).unwrap();
    let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    holder.set_nonblocking(true).unwrap();
    let address = holder.local_addr().unwrap();

    let control_a = AsyncListener::new(HandoffListener::bind(&paths.control).unwrap()).unwrap();
    let (stream_events, _) = tokio::sync::broadcast::channel(8);
    let worker = tokio::spawn(run_with(
        generation,
        paths.control.clone(),
        streaming_service(stream_events.clone()),
    ));
    let mut first = coordinator(&holder, paths.clone(), deployment).await;
    let channel_a = timed(
        "first worker attach",
        attach(&mut first, &control_a, generation, 1),
    )
    .await;
    let mut old_client = timed(
        "first connection handoff",
        connect_and_handoff(&mut first, address, &channel_a),
    )
    .await;
    round_trip(&mut old_client, b"before").await;
    let mut event_client = timed(
        "event stream handoff",
        connect_and_handoff(&mut first, address, &channel_a),
    )
    .await;
    start_event_stream(&mut event_client).await;

    drop(first);
    drop(channel_a);
    drop(control_a);
    std::fs::remove_file(&paths.control).unwrap();
    round_trip(&mut old_client, b"during").await;
    stream_events.send(b"data: during\n\n".to_vec()).unwrap();
    read_exact(&mut event_client, b"data: during\n\n").await;
    let queued_client = tokio::spawn(tokio::net::TcpStream::connect(address));

    let control_b = AsyncListener::new(HandoffListener::bind(&paths.control).unwrap()).unwrap();
    let state = load_state(&paths.state).unwrap();
    let mut second = coordinator(&holder, paths, state).await;
    let channel_b = timed(
        "replacement worker attach",
        attach(&mut second, &control_b, generation, 2),
    )
    .await;
    round_trip(&mut old_client, b"after").await;
    stream_events.send(b"data: after\n\n".to_vec()).unwrap();
    read_exact(&mut event_client, b"data: after\n\n").await;
    let mut gap_client = timed(
        "gap connection handoff",
        accept_queued_and_handoff(&mut second, queued_client, &channel_b),
    )
    .await;
    round_trip(&mut gap_client, b"accepted-during-gap").await;
    let mut new_client = timed(
        "replacement connection handoff",
        connect_and_handoff(&mut second, address, &channel_b),
    )
    .await;
    round_trip(&mut new_client, b"new").await;

    channel_b
        .send_control(&Control::Drain {
            generation: WireGenerationId::new(generation.get()),
        })
        .await
        .unwrap();
    timed("worker pause", wait_for_paused(&channel_b, generation)).await;
    drop(old_client);
    drop(event_client);
    drop(gap_client);
    drop(new_client);
    drop(stream_events);
    timed(
        "worker zero observation",
        wait_for_zero(&channel_b, generation),
    )
    .await;
    timed("worker exit", worker).await.unwrap().unwrap();
}

async fn timed<T>(label: &str, future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(STEP_TIMEOUT, future)
        .await
        .unwrap_or_else(|_| panic!("{label} timed out"))
}

async fn accept_queued_and_handoff(
    coordinator: &mut Coordinator,
    client: tokio::task::JoinHandle<std::io::Result<tokio::net::TcpStream>>,
    channel: &AsyncChannel,
) -> tokio::net::TcpStream {
    let (server, _) = coordinator.listener.accept().await.unwrap();
    let client = client.await.unwrap().unwrap();
    coordinator.handoff(server).await.unwrap();
    finish_handoff(coordinator, channel).await;
    client
}

async fn coordinator(
    holder: &std::net::TcpListener,
    paths: HostPaths,
    deployment: DeploymentState,
) -> Coordinator {
    let listener = tokio::net::TcpListener::from_std(holder.try_clone().unwrap()).unwrap();
    let mut coordinator = Coordinator::new(listener, paths, deployment).unwrap();
    coordinator.worker_command = Arc::new(|_, _| async { Ok(()) }.boxed());
    coordinator
}

async fn attach(
    coordinator: &mut Coordinator,
    listener: &AsyncListener,
    generation: GenerationId,
    registration: u64,
) -> Arc<AsyncChannel> {
    let channel = listener.accept().await.unwrap();
    assert!(matches!(
        channel.receive().await.unwrap(),
        Received::Control(Control::WorkerHello { generation: found, .. })
            if found.raw() == generation.get()
    ));
    channel
        .send_control(&Control::CoordinatorHello {
            epoch: coordinator.epoch(),
        })
        .await
        .unwrap();
    coordinator.workers.insert(
        generation.get(),
        WorkerSlot {
            registration,
            instance: WorkerInstanceId::new(registration).unwrap(),
            ready: false,
            accepting: false,
            draining: false,
            pending_reconciled: false,
            channel: channel.clone(),
        },
    );
    while !coordinator.accepts_clients() {
        let Received::Control(message) = channel.receive().await.unwrap() else {
            continue;
        };
        coordinator
            .handle_message(generation.get(), message)
            .await
            .unwrap();
        coordinator.advance().await.unwrap();
    }
    channel
}

async fn connect_and_handoff(
    coordinator: &mut Coordinator,
    address: std::net::SocketAddr,
    channel: &AsyncChannel,
) -> tokio::net::TcpStream {
    let client = tokio::spawn(tokio::net::TcpStream::connect(address));
    let (server, _) = coordinator.listener.accept().await.unwrap();
    let client = client.await.unwrap().unwrap();
    coordinator.handoff(server).await.unwrap();
    finish_handoff(coordinator, channel).await;
    client
}

async fn finish_handoff(coordinator: &mut Coordinator, channel: &AsyncChannel) {
    while !coordinator.pending.is_empty() {
        let Received::Control(message) = channel.receive().await.unwrap() else {
            continue;
        };
        coordinator
            .handle_message(coordinator.active.unwrap().get(), message)
            .await
            .unwrap();
    }
}

async fn wait_for_paused(channel: &AsyncChannel, generation: GenerationId) {
    loop {
        if matches!(
            channel.receive().await.unwrap(),
            Received::Control(Control::AdmissionsPaused { generation: found })
                if found.raw() == generation.get()
        ) {
            return;
        }
    }
}

async fn wait_for_zero(channel: &AsyncChannel, generation: GenerationId) {
    loop {
        let message = channel.receive().await.unwrap();
        if matches!(
            message,
            Received::Control(Control::ConnectionsObserved { generation: found, active: 0 })
                if found.raw() == generation.get()
        ) {
            return;
        }
    }
}

async fn round_trip(stream: &mut tokio::net::TcpStream, message: &[u8]) {
    timed("connection round trip", async {
        stream.write_all(message).await.unwrap();
        let mut found = vec![0; message.len()];
        stream.read_exact(&mut found).await.unwrap();
        assert_eq!(found, message);
    })
    .await;
}

async fn start_event_stream(stream: &mut tokio::net::TcpStream) {
    stream
        .write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    read_exact(
        stream,
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: ready\n\n",
    )
    .await;
}

async fn read_exact(stream: &mut tokio::net::TcpStream, expected: &[u8]) {
    timed("event stream read", async {
        let mut found = vec![0; expected.len()];
        stream.read_exact(&mut found).await.unwrap();
        assert_eq!(found, expected);
    })
    .await;
}

fn streaming_service(events: tokio::sync::broadcast::Sender<Vec<u8>>) -> Service {
    Arc::new(move |mut stream, lifetime| {
        let mut events = events.subscribe();
        async move {
            let _lifetime = lifetime;
            let mut first = [0_u8; 1];
            if stream.read_exact(&mut first).await.is_err() {
                return;
            }
            if first == *b"G" {
                let mut method = [0_u8; 3];
                if stream.read_exact(&mut method).await.is_err() || &method != b"ET " {
                    return;
                }
                let mut request = Vec::new();
                let mut byte = [0_u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    request.push(byte[0]);
                }
                if stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: ready\n\n",
                    )
                    .await
                    .is_err()
                {
                    return;
                }
                let mut closed = [0_u8; 1];
                loop {
                    tokio::select! {
                        event = events.recv() => match event {
                            Ok(event) if stream.write_all(&event).await.is_ok() => {}
                            _ => break,
                        },
                        read = stream.read(&mut closed) => {
                            if !matches!(read, Ok(size) if size > 0) {
                                break;
                            }
                        }
                    }
                }
                return;
            }
            if stream.write_all(&first).await.is_err() {
                return;
            }
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

fn test_paths(root: &std::path::Path) -> HostPaths {
    let executable = root.join("test-router");
    std::fs::write(&executable, b"synthetic router executable").unwrap();
    HostPaths {
        executable,
        generations: root.join("generations"),
        control: root.join("handoff.sock"),
        state: root.join("state.json"),
    }
}

fn active_deployment(paths: &HostPaths) -> (DeploymentState, GenerationId) {
    let mut state = DeploymentState::default();
    let DeployPlan::StageTarget { target, .. } =
        state.plan_deploy(paths.build_id().unwrap()).unwrap()
    else {
        panic!("expected staged generation")
    };
    for event in [
        DeploymentEvent::Prepared { target },
        DeploymentEvent::PreviousPaused { target },
        DeploymentEvent::TargetAccepting { target },
    ] {
        state.reconcile(event).unwrap();
    }
    (state, target)
}
