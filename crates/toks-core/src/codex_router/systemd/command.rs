use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use super::plan::Action;
use crate::codex_router::proxy::HEALTH_BODY;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

mod process;
pub(in crate::codex_router) use process::coordinator_main_pid_until;
#[cfg(test)]
use process::process_matches;
pub(super) use process::{coordinator_matches_until, resume_matches_until};

pub(super) fn execute_until(action: Action, deadline: Instant) -> Result<()> {
    systemctl_until(action.systemctl_args(), deadline)
}

pub(super) fn is_unit_active(name: &str) -> bool {
    is_unit_active_until(name, command_deadline()).unwrap_or(false)
}

pub(super) fn is_unit_active_until(name: &str, deadline: Instant) -> Result<bool> {
    active_status(systemctl_output_until(
        &["is-active", "--quiet", name],
        deadline,
    )?)
}

fn active_status(output: Output) -> Result<bool> {
    match output.status.code() {
        Some(0) => Ok(true),
        Some(3 | 4) => Ok(false),
        _ => anyhow::bail!(
            "systemctl active-state query failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

pub(super) fn health_check(address: SocketAddr) -> Result<()> {
    health_check_until(address, Instant::now() + Duration::from_millis(250))
}

pub(super) fn health_check_until(address: SocketAddr, deadline: Instant) -> Result<()> {
    let mut stream = TcpStream::connect_timeout(&address, remaining(deadline)?)?;
    stream.set_write_timeout(Some(remaining(deadline)?))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = Vec::with_capacity(256);
    let mut buffer = [0_u8; 256];
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                anyhow::ensure!(
                    response.len() <= 4096,
                    "router health response is too large"
                );
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                anyhow::bail!("router health check timed out at its deadline")
            }
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::ensure!(healthy_response(&response), "unhealthy router service");
    Ok(())
}

fn remaining(deadline: Instant) -> Result<Duration> {
    let duration = deadline.saturating_duration_since(Instant::now());
    anyhow::ensure!(!duration.is_zero(), "router health check deadline expired");
    Ok(duration)
}

pub(crate) fn healthy_response(response: &[u8]) -> bool {
    response.starts_with(b"HTTP/1.1 200") && response.ends_with(HEALTH_BODY.as_bytes())
}

fn systemctl_until(args: &[&str], deadline: Instant) -> Result<()> {
    let output = systemctl_output_until(args, deadline)?;
    if output.status.success() {
        return Ok(());
    }
    anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

pub(super) fn systemctl_stdout_until(args: &[&str], deadline: Instant) -> Result<String> {
    let output = systemctl_output_until(args, deadline)?;
    anyhow::ensure!(output.status.success(), "systemctl query failed");
    String::from_utf8(output.stdout).context("systemctl returned invalid UTF-8")
}

fn systemctl_output_until(args: &[&str], deadline: Instant) -> Result<Output> {
    let mut command = Command::new("systemctl");
    command.arg("--user").args(args);
    bounded_output_until(&mut command, deadline).context("running systemctl --user")
}

fn command_deadline() -> Instant {
    Instant::now() + COMMAND_TIMEOUT
}

fn bounded_output_until(command: &mut Command, deadline: Instant) -> Result<Output> {
    anyhow::ensure!(
        Instant::now() < deadline,
        "systemd control command timed out"
    );
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map_err(Into::into);
        }
        let now = Instant::now();
        if now >= deadline {
            let kill = child.kill();
            let reap = child.wait();
            if let Err(error) = kill {
                anyhow::bail!("systemd control command timed out and could not be killed: {error}");
            }
            if let Err(error) = reap {
                anyhow::bail!("systemd control command timed out and could not be reaped: {error}");
            }
            anyhow::bail!("systemd control command timed out");
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(10)));
    }
}

#[cfg(test)]
mod tests;
