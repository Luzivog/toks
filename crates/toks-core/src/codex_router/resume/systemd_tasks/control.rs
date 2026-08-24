use std::ffi::OsString;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn execute(arguments: Vec<OsString>) -> Result<()> {
    let mut command = Command::new("systemd-run");
    command.arg("--user").args(arguments);
    checked(command, "launching resumed task unit")
}

pub(super) fn checked(command: Command, context: &'static str) -> Result<()> {
    let output = bounded_output(command).context(context)?;
    anyhow::ensure!(
        output.status.success(),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

pub(super) fn checked_allow_not_found(command: Command, context: &'static str) -> Result<()> {
    let output = bounded_output(command).context(context)?;
    anyhow::ensure!(
        output.status.success() || output.status.code() == Some(5),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

pub(super) fn bounded_output(mut command: Command) -> Result<Output> {
    bounded_output_with_timeout(&mut command, COMMAND_TIMEOUT)
}

pub(super) fn bounded_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn()?;
    let stdout = drain(child.stdout.take().context("capturing command stdout")?);
    let stderr = drain(child.stderr.take().context("capturing command stderr")?);
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = child.kill();
            break (child.wait()?, true);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("stdout drain panicked"))??;
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("stderr drain panicked"))??;
    if timed_out {
        anyhow::bail!(
            "task-unit control command timed out; stdout: {}; stderr: {}",
            String::from_utf8_lossy(&stdout).trim(),
            String::from_utf8_lossy(&stderr).trim()
        );
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn drain(
    mut pipe: impl Read + Send + 'static,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}
