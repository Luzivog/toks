use super::protocol::{encode, Connection, Control, WireMessage, MAX_PACKET_BYTES};
use super::{HandoffError, PeerIdentity, Received};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::socket::{
    accept4, bind, connect, getsockopt, listen, recvmsg, sendmsg, socket, sockopt, AddressFamily,
    Backlog, ControlMessage, ControlMessageOwned, MsgFlags, SockFlag, SockType, UnixAddr,
};
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const SOCKET_FLAGS: SockFlag = SockFlag::SOCK_CLOEXEC.union(SockFlag::SOCK_NONBLOCK);
pub(super) const MAX_RIGHTS_PER_PACKET: usize = 253;

pub(crate) struct HandoffListener(OwnedFd);

impl AsFd for HandoffListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for HandoffListener {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl HandoffListener {
    pub fn bind(path: &Path) -> Result<Self, HandoffError> {
        let backlog = Backlog::new(32)?;
        let fd = socket(AddressFamily::Unix, SockType::SeqPacket, SOCKET_FLAGS, None)?;
        bind(fd.as_raw_fd(), &UnixAddr::new(path)?)?;
        if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            let _ = std::fs::remove_file(path);
            return Err(error.into());
        }
        if let Err(error) = listen(&fd, backlog) {
            let _ = std::fs::remove_file(path);
            return Err(error.into());
        }
        Ok(Self(fd))
    }

    pub fn accept(&self) -> Result<HandoffChannel, HandoffError> {
        let fd = accept4(self.0.as_raw_fd(), SOCKET_FLAGS)?;
        // SAFETY: accept4 returned a new descriptor owned by this call.
        HandoffChannel::from_fd(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

pub(crate) struct HandoffChannel {
    fd: OwnedFd,
    peer: PeerIdentity,
}

impl AsFd for HandoffChannel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl AsRawFd for HandoffChannel {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl HandoffChannel {
    pub fn connect(path: &Path) -> Result<Self, HandoffError> {
        let fd = socket(
            AddressFamily::Unix,
            SockType::SeqPacket,
            SockFlag::SOCK_CLOEXEC,
            None,
        )?;
        connect(fd.as_raw_fd(), &UnixAddr::new(path)?)?;
        set_nonblocking(fd.as_raw_fd())?;
        Self::from_fd(fd)
    }

    pub fn peer_identity(&self) -> PeerIdentity {
        self.peer
    }

    pub fn send_control(&self, control: &Control) -> Result<(), HandoffError> {
        self.send(WireMessage::Control(control.clone()), &[])
    }

    pub fn send_connection(
        &self,
        connection: Connection,
        socket: BorrowedFd<'_>,
    ) -> Result<(), HandoffError> {
        self.send(WireMessage::Connection(connection), &[socket.as_raw_fd()])
    }

    pub fn receive(&self) -> Result<Received, HandoffError> {
        receive(self.fd.as_raw_fd())
    }

    pub(super) fn from_fd(fd: OwnedFd) -> Result<Self, HandoffError> {
        set_nonblocking(fd.as_raw_fd())?;
        let credentials = getsockopt(&fd, sockopt::PeerCredentials)?;
        Ok(Self {
            fd,
            peer: PeerIdentity {
                pid: credentials.pid(),
                uid: credentials.uid(),
            },
        })
    }

    #[cfg(test)]
    pub(super) fn raw_fd(&self) -> RawFd {
        self.as_raw_fd()
    }

    pub(super) fn send(&self, message: WireMessage, fds: &[RawFd]) -> Result<(), HandoffError> {
        let bytes = encode(message)?;
        let iov = [IoSlice::new(&bytes)];
        let cmsgs = (!fds.is_empty()).then_some(ControlMessage::ScmRights(fds));
        let sent = sendmsg::<()>(
            self.fd.as_raw_fd(),
            &iov,
            cmsgs.as_slice(),
            MsgFlags::MSG_NOSIGNAL,
            None,
        )?;
        (sent == bytes.len())
            .then_some(())
            .ok_or(HandoffError::Truncated)
    }
}

fn set_nonblocking(fd: RawFd) -> Result<(), HandoffError> {
    let flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    Ok(())
}

fn receive(fd: RawFd) -> Result<Received, HandoffError> {
    let mut bytes = [0_u8; MAX_PACKET_BYTES + 1];
    let mut ancillary = nix::cmsg_space!([RawFd; MAX_RIGHTS_PER_PACKET]);
    let (size, flags, received_fds) = {
        let mut iov = [IoSliceMut::new(&mut bytes)];
        let message = recvmsg::<()>(
            fd,
            &mut iov,
            Some(&mut ancillary),
            MsgFlags::MSG_CMSG_CLOEXEC | MsgFlags::MSG_DONTWAIT,
        )?;
        let mut received_fds = Vec::new();
        for control in message.cmsgs().map_err(|_| HandoffError::Malformed)? {
            match control {
                ControlMessageOwned::ScmRights(raw_fds) => {
                    received_fds.extend(raw_fds.into_iter().map(|fd| {
                        // SAFETY: SCM_RIGHTS installed a new descriptor owned by this process.
                        unsafe { OwnedFd::from_raw_fd(fd) }
                    }))
                }
                _ => return Err(HandoffError::Malformed),
            }
        }
        (message.bytes, message.flags, received_fds)
    };
    if size == 0 {
        return Err(HandoffError::Closed);
    }
    if flags.intersects(MsgFlags::MSG_TRUNC | MsgFlags::MSG_CTRUNC) {
        return Err(HandoffError::Truncated);
    }
    let message = super::protocol::decode(&bytes[..size])?;
    match (message, received_fds.len()) {
        (WireMessage::Control(control), 0) => Ok(Received::Control(control)),
        (WireMessage::Control(_), _) => Err(HandoffError::UnexpectedFd),
        (WireMessage::Connection(_), 0) => Err(HandoffError::MissingFd),
        (WireMessage::Connection(connection), 1) => {
            let fd = received_fds
                .into_iter()
                .next()
                .expect("checked descriptor count");
            set_nonblocking(fd.as_raw_fd())?;
            Ok(Received::Connection(connection, fd))
        }
        (WireMessage::Connection(_), _) => Err(HandoffError::MultipleFds),
    }
}
