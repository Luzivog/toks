use super::protocol::{WireMessage, MAX_PACKET_BYTES};
use super::transport::MAX_RIGHTS_PER_PACKET;
use super::*;
use nix::fcntl::{fcntl, FcntlArg, FdFlag, OFlag};
use nix::sys::socket::{
    sendmsg, socketpair, AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType,
};
use std::io::IoSlice;
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;

fn channels() -> (HandoffChannel, HandoffChannel) {
    let (left, right) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
    )
    .unwrap();
    (
        HandoffChannel::from_fd(left).unwrap(),
        HandoffChannel::from_fd(right).unwrap(),
    )
}

#[test]
fn listener_connects_with_kernel_authenticated_peer_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.sock");
    let listener = HandoffListener::bind(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let client = HandoffChannel::connect(&path).unwrap();
    let server = listener.accept().unwrap();

    let expected = PeerIdentity {
        pid: std::process::id() as i32,
        uid: nix::sys::socket::UnixCredentials::new().uid(),
    };
    assert_eq!(client.peer_identity(), expected);
    assert_eq!(server.peer_identity(), expected);
}

#[test]
fn control_packets_round_trip_without_descriptors() {
    let (sender, receiver) = channels();
    let generation = GenerationId::new(12);
    let controls = [
        Control::WorkerHello {
            generation,
            instance: WorkerInstanceId::new(7).unwrap(),
        },
        Control::CoordinatorHello { epoch: 91 },
        Control::Ready { generation },
        Control::Activate { generation },
        Control::Drain { generation },
    ];

    assert_eq!(generation.raw(), 12);
    for control in controls {
        sender.send_control(&control).unwrap();
        assert!(matches!(receiver.receive(), Ok(Received::Control(found)) if found == control));
    }
}

#[test]
fn connection_ack_round_trips_with_the_handoff_id() {
    let (sender, receiver) = channels();
    let handoff_id = HandoffId::new(7, 67);
    for control in [
        Control::ConnectionAck { handoff_id },
        Control::ConnectionCommitted { handoff_id },
        Control::ConnectionCommitAck { handoff_id },
        Control::ConnectionFinalized { handoff_id },
        Control::ConnectionFinalizedAck { handoff_id },
    ] {
        sender.send_control(&control).unwrap();
        assert!(matches!(receiver.receive(), Ok(Received::Control(found)) if found == control));
    }
}

#[test]
fn restart_safe_handoff_id_has_a_stable_wire_shape() {
    let handoff_id = HandoffId::new(7, 42);
    assert_eq!(handoff_id.coordinator_epoch(), 7);
    assert_eq!(handoff_id.sequence(), 42);

    let connection = super::protocol::encode(WireMessage::Connection(Connection {
        handoff_id,
        duplicate: false,
    }))
    .unwrap();
    let ack = super::protocol::encode(WireMessage::Control(Control::ConnectionAck { handoff_id }))
        .unwrap();

    assert_eq!(
        String::from_utf8(connection).unwrap(),
        r#"{"version":1,"message":{"handoff_id":{"coordinator_epoch":7,"sequence":42},"duplicate":false}}"#
    );
    assert_eq!(
        String::from_utf8(ack).unwrap(),
        r#"{"version":1,"message":{"type":"connection_ack","handoff_id":{"coordinator_epoch":7,"sequence":42}}}"#
    );
}

#[test]
fn observed_connection_count_round_trips_for_retirement() {
    let (worker, coordinator) = channels();
    let observed = Control::ConnectionsObserved {
        generation: GenerationId::new(23),
        active: 0,
    };

    worker.send_control(&observed).unwrap();

    assert!(matches!(coordinator.receive(), Ok(Received::Control(found)) if found == observed));
}

#[test]
fn admission_barrier_acknowledgements_round_trip() {
    let (worker, coordinator) = channels();
    let generation = GenerationId::new(24);
    let acknowledgements = [
        Control::AdmissionsPaused { generation },
        Control::Accepting { generation },
    ];

    for acknowledgement in acknowledgements {
        worker.send_control(&acknowledgement).unwrap();
        assert!(matches!(
            coordinator.receive(),
            Ok(Received::Control(found)) if found == acknowledgement
        ));
    }
}

#[test]
fn connection_transfers_exactly_one_nonblocking_cloexec_descriptor() {
    let (sender, receiver) = channels();
    let (transferred, _peer) = socketpair(
        AddressFamily::Unix,
        SockType::Stream,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let connection = Connection {
        handoff_id: HandoffId::new(7, 41),
        duplicate: false,
    };
    let original_status =
        OFlag::from_bits_truncate(fcntl(transferred.as_raw_fd(), FcntlArg::F_GETFL).unwrap());
    assert!(!original_status.contains(OFlag::O_NONBLOCK));

    sender
        .send_connection(connection, transferred.as_fd())
        .unwrap();
    let Received::Connection(found, fd) = receiver.receive().unwrap() else {
        panic!("expected a connection handoff");
    };

    assert_eq!(found, connection);
    let status = OFlag::from_bits_truncate(fcntl(fd.as_raw_fd(), FcntlArg::F_GETFL).unwrap());
    let descriptor = FdFlag::from_bits_truncate(fcntl(fd.as_raw_fd(), FcntlArg::F_GETFD).unwrap());
    assert!(status.contains(OFlag::O_NONBLOCK));
    assert!(descriptor.contains(FdFlag::FD_CLOEXEC));
}

#[test]
fn duplicate_metadata_survives_retry_handoff() {
    let (sender, receiver) = channels();
    let (transferred, _peer) = socketpair(
        AddressFamily::Unix,
        SockType::Stream,
        None,
        SockFlag::SOCK_NONBLOCK,
    )
    .unwrap();
    let retry = Connection {
        handoff_id: HandoffId::new(8, 99),
        duplicate: true,
    };

    sender.send_connection(retry, transferred.as_fd()).unwrap();

    assert!(matches!(receiver.receive(), Ok(Received::Connection(found, _)) if found == retry));
}

#[test]
fn sender_keeps_its_original_descriptor_until_ack() {
    let (coordinator, worker) = channels();
    let (accepted, _client) = socketpair(
        AddressFamily::Unix,
        SockType::Stream,
        None,
        SockFlag::SOCK_NONBLOCK,
    )
    .unwrap();
    let handoff_id = HandoffId::new(8, 101);

    coordinator
        .send_connection(
            Connection {
                handoff_id,
                duplicate: false,
            },
            accepted.as_fd(),
        )
        .unwrap();
    let Received::Connection(_, worker_copy) = worker.receive().unwrap() else {
        panic!("expected a connection handoff");
    };
    assert!(fcntl(accepted.as_raw_fd(), FcntlArg::F_GETFD).is_ok());

    worker
        .send_control(&Control::ConnectionAck { handoff_id })
        .unwrap();
    assert!(matches!(
        coordinator.receive(),
        Ok(Received::Control(Control::ConnectionAck { handoff_id: found })) if found == handoff_id
    ));
    assert!(fcntl(accepted.as_raw_fd(), FcntlArg::F_GETFD).is_ok());
    drop(worker_copy);
}

#[test]
fn connection_packet_without_fd_is_rejected() {
    let (sender, receiver) = channels();
    sender
        .send(
            WireMessage::Connection(Connection {
                handoff_id: HandoffId::new(9, 1),
                duplicate: false,
            }),
            &[],
        )
        .unwrap();

    assert!(matches!(receiver.receive(), Err(HandoffError::MissingFd)));
}

#[test]
fn control_packet_with_fd_is_rejected() {
    let (sender, receiver) = channels();
    let (transferred, _peer) = socketpair(
        AddressFamily::Unix,
        SockType::Stream,
        None,
        SockFlag::empty(),
    )
    .unwrap();
    sender
        .send(
            WireMessage::Control(Control::Ready {
                generation: GenerationId::new(2),
            }),
            &[transferred.as_raw_fd()],
        )
        .unwrap();

    assert!(matches!(
        receiver.receive(),
        Err(HandoffError::UnexpectedFd)
    ));
}

#[test]
fn multiple_fds_are_rejected() {
    let (sender, receiver) = channels();
    let (first, second) = socketpair(
        AddressFamily::Unix,
        SockType::Stream,
        None,
        SockFlag::empty(),
    )
    .unwrap();
    sender
        .send(
            WireMessage::Connection(Connection {
                handoff_id: HandoffId::new(9, 3),
                duplicate: false,
            }),
            &[first.as_raw_fd(), second.as_raw_fd()],
        )
        .unwrap();

    assert!(matches!(receiver.receive(), Err(HandoffError::MultipleFds)));
}

#[test]
fn many_descriptors_are_received_and_rejected_without_truncation() {
    let (sender, receiver) = channels();
    let (transferred, _peer) = socketpair(
        AddressFamily::Unix,
        SockType::Stream,
        None,
        SockFlag::empty(),
    )
    .unwrap();
    let excess = [transferred.as_raw_fd(); MAX_RIGHTS_PER_PACKET];
    sender
        .send(
            WireMessage::Connection(Connection {
                handoff_id: HandoffId::new(9, 4),
                duplicate: false,
            }),
            &excess,
        )
        .unwrap();

    assert!(matches!(receiver.receive(), Err(HandoffError::MultipleFds)));
}

#[test]
fn malformed_unknown_and_wrong_version_packets_are_rejected() {
    let cases = [
        (b"not-json".as_slice(), HandoffError::Malformed),
        (
            br#"{"version":1,"message":{"type":"future","generation":1}}"#,
            HandoffError::Malformed,
        ),
        (
            br#"{"version":1,"message":{"type":"ready","generation":1},"future":true}"#,
            HandoffError::Malformed,
        ),
        (
            br#"{"version":9,"message":{"type":"ready","generation":1}}"#,
            HandoffError::UnsupportedVersion(9),
        ),
    ];

    for (bytes, expected) in cases {
        let (sender, receiver) = channels();
        raw_send(&sender, bytes, &[]);
        let error = receiver.receive().unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
    }
}

#[test]
fn empty_nonblocking_channel_reports_would_block_and_closed_peer_reports_closed() {
    let (sender, receiver) = channels();
    assert!(matches!(
        receiver.receive(),
        Err(HandoffError::System(nix::errno::Errno::EAGAIN))
    ));

    drop(sender);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match receiver.receive() {
            Err(HandoffError::Closed) => break,
            Err(HandoffError::System(nix::errno::Errno::EAGAIN))
                if std::time::Instant::now() < deadline =>
            {
                std::thread::yield_now();
            }
            result => panic!("closed peer did not become observable: {result:?}"),
        }
    }
}

#[test]
fn oversized_packet_is_rejected_before_parsing() {
    let (sender, receiver) = channels();
    raw_send(&sender, &vec![b'x'; MAX_PACKET_BYTES + 1], &[]);

    assert!(matches!(
        receiver.receive(),
        Err(HandoffError::Oversized(size)) if size == MAX_PACKET_BYTES + 1
    ));
}

fn raw_send(channel: &HandoffChannel, bytes: &[u8], fds: &[RawFd]) {
    let iov = [IoSlice::new(bytes)];
    let cmsgs = (!fds.is_empty()).then_some(ControlMessage::ScmRights(fds));
    let sent = sendmsg::<()>(
        channel.raw_fd(),
        &iov,
        cmsgs.as_slice(),
        MsgFlags::MSG_NOSIGNAL,
        None,
    )
    .unwrap();
    assert_eq!(sent, bytes.len());
}
