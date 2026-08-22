use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::{proxy::HEALTH_BODY, ROUTER_PORT};

const UNIT_NAME: &str = "toks-router.service";

pub(super) fn install(executable: &Path, codex_executable: &Path) -> Result<()> {
    let path = unit_path()?;
    let unit = render_unit(executable, codex_executable);
    if fs::read_to_string(&path).ok().as_deref() != Some(unit.as_str()) {
        crate::rotation::write_private_atomic(&path, unit.as_bytes(), "router user service")?;
    }
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT_NAME])
}

pub(super) fn uninstall() -> Result<()> {
    let path = unit_path()?;
    let _ = systemctl(&["disable", "--now", UNIT_NAME]);
    if path.exists() {
        fs::remove_file(path).context("removing Toks router user service")?;
    }
    systemctl(&["daemon-reload"])
}

pub(super) fn is_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", UNIT_NAME])
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn wait_until_ready() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let address = SocketAddr::from(([127, 0, 0, 1], ROUTER_PORT));
    while Instant::now() < deadline {
        if health_check(address).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("Toks router did not become ready")
}

pub(super) fn unit_path() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|root| root.join("systemd/user").join(UNIT_NAME))
        .context("no local configuration directory")
}

pub(super) fn render_unit(executable: &Path, codex_executable: &Path) -> String {
    let executable = quote(executable);
    let codex_environment = environment("TOKS_CODEX_BIN", &codex_executable.display().to_string());
    let inherited_environment = ["PATH", "CODEX_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME"]
        .into_iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| environment(name, &value))
        })
        .map(|value| format!("\nEnvironment={value}"))
        .collect::<String>();
    format!(
        "[Unit]\nDescription=Toks Codex account router\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={executable}\nEnvironment={codex_environment}{inherited_environment}\nRestart=on-failure\nRestartSec=2\nUMask=0077\nNoNewPrivileges=true\nProtectSystem=full\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nLockPersonality=true\n\n[Install]\nWantedBy=default.target\n"
    )
}

fn environment(name: &str, value: &str) -> String {
    let escaped = value
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{name}={escaped}\"")
}

fn quote(path: &Path) -> String {
    let escaped = path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn systemctl(args: &[&str]) -> Result<()> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .context("running systemctl --user")?;
    if output.status.success() {
        return Ok(());
    }
    anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

fn health_check(address: SocketAddr) -> Result<()> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(250))?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = Vec::with_capacity(256);
    stream.read_to_end(&mut response)?;
    anyhow::ensure!(healthy_response(&response), "unhealthy router service");
    Ok(())
}

pub(super) fn healthy_response(response: &[u8]) -> bool {
    response.starts_with(b"HTTP/1.1 200") && response.ends_with(HEALTH_BODY.as_bytes())
}
