use nix::errno::Errno;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::os::fd::OwnedFd;

use super::identity::{GenerationId, HandoffId, WorkerInstanceId};

pub(super) const MAX_PACKET_BYTES: usize = 4_096;
const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Control {
    WorkerHello {
        generation: GenerationId,
        instance: WorkerInstanceId,
    },
    CoordinatorHello {
        epoch: u64,
    },
    Ready {
        generation: GenerationId,
    },
    Activate {
        generation: GenerationId,
    },
    Drain {
        generation: GenerationId,
    },
    AdmissionsPaused {
        generation: GenerationId,
    },
    Accepting {
        generation: GenerationId,
    },
    ConnectionAck {
        handoff_id: HandoffId,
    },
    ConnectionCommitted {
        handoff_id: HandoffId,
    },
    ConnectionCommitAck {
        handoff_id: HandoffId,
    },
    ConnectionFinalized {
        handoff_id: HandoffId,
    },
    ConnectionFinalizedAck {
        handoff_id: HandoffId,
    },
    ConnectionsObserved {
        generation: GenerationId,
        active: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Connection {
    pub handoff_id: HandoffId,
    /// True only when retrying an unacknowledged handoff to the same worker.
    pub duplicate: bool,
}

#[derive(Debug)]
pub(crate) enum Received {
    Control(Control),
    Connection(Connection, OwnedFd),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PeerIdentity {
    pub pid: i32,
    pub uid: u32,
}

#[derive(Debug)]
pub(crate) enum HandoffError {
    System(Errno),
    Io(std::io::Error),
    Malformed,
    Oversized(usize),
    UnsupportedVersion(u16),
    Closed,
    MissingFd,
    UnexpectedFd,
    MultipleFds,
    Truncated,
}

impl fmt::Display for HandoffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System(error) => write!(f, "handoff socket failed: {error}"),
            Self::Io(error) => write!(f, "handoff socket filesystem failed: {error}"),
            Self::Malformed => f.write_str("malformed handoff packet"),
            Self::Oversized(size) => write!(f, "handoff packet is too large: {size} bytes"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported handoff version {version}")
            }
            Self::Closed => f.write_str("handoff socket closed"),
            Self::MissingFd => f.write_str("connection handoff omitted its descriptor"),
            Self::UnexpectedFd => f.write_str("control packet included a descriptor"),
            Self::MultipleFds => f.write_str("handoff packet included multiple descriptors"),
            Self::Truncated => f.write_str("handoff packet or ancillary data was truncated"),
        }
    }
}

impl std::error::Error for HandoffError {}

impl From<Errno> for HandoffError {
    fn from(error: Errno) -> Self {
        Self::System(error)
    }
}

impl From<std::io::Error> for HandoffError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u16,
    message: WireMessage,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum WireMessage {
    Control(Control),
    Connection(Connection),
}

pub(super) fn encode(message: WireMessage) -> Result<Vec<u8>, HandoffError> {
    let bytes = serde_json::to_vec(&Envelope {
        version: PROTOCOL_VERSION,
        message,
    })
    .map_err(|_| HandoffError::Malformed)?;
    if bytes.len() > MAX_PACKET_BYTES {
        return Err(HandoffError::Oversized(bytes.len()));
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<WireMessage, HandoffError> {
    if bytes.len() > MAX_PACKET_BYTES {
        return Err(HandoffError::Oversized(bytes.len()));
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| HandoffError::Malformed)?;
    if envelope.version != PROTOCOL_VERSION {
        return Err(HandoffError::UnsupportedVersion(envelope.version));
    }
    Ok(envelope.message)
}
