use anyhow::{bail, Context, Result};
use nix::fcntl::{fcntl, FcntlArg, FdFlag, OFlag};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::os::fd::{FromRawFd, RawFd};

use crate::codex_router::ROUTER_PORT;

const SYSTEMD_FD_START: RawFd = 3;

pub(super) struct Activation {
    values: HashMap<String, String>,
    pid: u32,
}

impl Activation {
    pub(super) fn read() -> Self {
        Self {
            values: ["LISTEN_PID", "LISTEN_FDS", "LISTEN_FDNAMES"]
                .into_iter()
                .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
                .collect(),
            pid: std::process::id(),
        }
    }

    #[cfg(test)]
    pub(super) fn new(values: &[(&str, &str)], pid: u32) -> Self {
        Self {
            values: values
                .iter()
                .map(|(key, value)| ((*key).into(), (*value).into()))
                .collect(),
            pid,
        }
    }

    pub(super) fn descriptor(&self) -> Result<RawFd> {
        let listen_pid = self.parse("LISTEN_PID")?;
        let listen_fds = self.parse("LISTEN_FDS")?;
        if listen_pid != u64::from(self.pid) {
            bail!("socket activation belongs to PID {listen_pid}, not this process");
        }
        if listen_fds != 1 {
            bail!("expected exactly one activated listener, found {listen_fds}");
        }
        if self.value("LISTEN_FDNAMES")? != "router" {
            bail!("activated listener is not named router");
        }
        Ok(SYSTEMD_FD_START)
    }

    fn parse(&self, key: &str) -> Result<u64> {
        self.value(key)?
            .parse()
            .with_context(|| format!("invalid {key}"))
    }

    fn value(&self, key: &str) -> Result<&str> {
        self.values
            .get(key)
            .map(String::as_str)
            .with_context(|| format!("missing {key}"))
    }
}

pub(super) fn systemd_listener() -> Result<tokio::net::TcpListener> {
    let descriptor = Activation::read().descriptor()?;
    set_descriptor_flags(descriptor)?;
    // SAFETY: strict systemd activation validation established sole ownership of fd 3.
    let listener = unsafe { std::net::TcpListener::from_raw_fd(descriptor) };
    let expected = SocketAddr::from((Ipv4Addr::LOCALHOST, ROUTER_PORT));
    anyhow::ensure!(
        listener.local_addr()? == expected,
        "activated listener has wrong address"
    );
    listener.set_nonblocking(true)?;
    tokio::net::TcpListener::from_std(listener).context("adopting activated router listener")
}

fn set_descriptor_flags(fd: RawFd) -> Result<()> {
    let status = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(status | OFlag::O_NONBLOCK))?;
    let descriptor = FdFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFD)?);
    fcntl(fd, FcntlArg::F_SETFD(descriptor | FdFlag::FD_CLOEXEC))?;
    Ok(())
}
