//! Private coordinator-to-worker transport for transferring accepted sockets.

mod identity;
mod protocol;
mod transport;

pub(crate) use identity::{GenerationId, HandoffId, WorkerInstanceId};
pub(crate) use protocol::{Connection, Control, HandoffError, PeerIdentity, Received};
pub(crate) use transport::{HandoffChannel, HandoffListener};

#[cfg(test)]
mod tests;
