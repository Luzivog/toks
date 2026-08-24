use std::sync::Arc;

use tokio::io::unix::AsyncFd;

use crate::codex_router::handoff::{
    Connection, Control, HandoffChannel, HandoffError, HandoffListener, Received,
};

pub(super) struct AsyncChannel(AsyncFd<HandoffChannel>);

impl AsyncChannel {
    pub(super) fn new(channel: HandoffChannel) -> std::io::Result<Self> {
        AsyncFd::new(channel).map(Self)
    }

    pub(super) fn peer_identity(&self) -> crate::codex_router::handoff::PeerIdentity {
        self.0.get_ref().peer_identity()
    }

    pub(super) async fn receive(&self) -> std::io::Result<Received> {
        loop {
            let mut ready = self.0.readable().await?;
            match ready.try_io(|inner| map_io(inner.get_ref().receive())) {
                Ok(result) => return result,
                Err(_) => continue,
            }
        }
    }

    pub(super) async fn send_control(&self, message: &Control) -> std::io::Result<()> {
        loop {
            let mut ready = self.0.writable().await?;
            match ready.try_io(|inner| map_io(inner.get_ref().send_control(message))) {
                Ok(result) => return result,
                Err(_) => continue,
            }
        }
    }

    pub(super) async fn send_connection(
        &self,
        message: Connection,
        socket: &tokio::net::TcpStream,
    ) -> std::io::Result<()> {
        loop {
            let mut ready = self.0.writable().await?;
            match ready
                .try_io(|inner| map_io(inner.get_ref().send_connection(message, socket.as_fd())))
            {
                Ok(result) => return result,
                Err(_) => continue,
            }
        }
    }

    #[cfg(test)]
    pub(super) fn fill_send_buffer(&self) {
        let message = Control::ConnectionFinalized {
            handoff_id: crate::codex_router::handoff::HandoffId::new(1, 1),
        };
        loop {
            match self.0.get_ref().send_control(&message) {
                Err(HandoffError::System(nix::errno::Errno::EAGAIN)) => return,
                Ok(()) => {}
                Err(error) => panic!("filling handoff send buffer failed: {error}"),
            }
        }
    }
}

pub(super) struct AsyncListener(AsyncFd<HandoffListener>);

impl AsyncListener {
    pub(super) fn new(listener: HandoffListener) -> std::io::Result<Self> {
        AsyncFd::new(listener).map(Self)
    }

    pub(super) async fn accept(&self) -> std::io::Result<Arc<AsyncChannel>> {
        loop {
            let mut ready = self.0.readable().await?;
            match ready.try_io(|inner| map_io(inner.get_ref().accept())) {
                Ok(Ok(channel)) => return Ok(Arc::new(AsyncChannel::new(channel)?)),
                Ok(Err(error)) => return Err(error),
                Err(_) => continue,
            }
        }
    }
}

fn map_io<T>(result: Result<T, HandoffError>) -> std::io::Result<T> {
    result.map_err(|error| match error {
        HandoffError::System(nix::errno::Errno::EAGAIN) => std::io::ErrorKind::WouldBlock.into(),
        other => std::io::Error::other(other),
    })
}

use std::os::fd::AsFd;
