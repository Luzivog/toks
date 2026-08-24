use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::process::Command;

use crate::codex_router::host::GenerationId;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::codex_router::host::process) enum Liveness {
    Running,
    Stopped,
    Unknown,
}

pub(super) async fn run(action: &str, generations: Vec<GenerationId>) -> Result<()> {
    if generations.is_empty() {
        return Ok(());
    }
    let mut command = Command::new("systemctl");
    command.args(["--user", action]);
    command.args(generations.iter().map(unit_name));
    let output = output_with_timeout(command, COMMAND_TIMEOUT).await?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[cfg(not(test))]
pub(super) async fn inventory() -> Result<BTreeMap<GenerationId, Liveness>> {
    let mut command = Command::new("systemctl");
    command.args([
        "--user",
        "list-units",
        "--all",
        "--full",
        "--plain",
        "--no-legend",
        "--no-pager",
        "toks-router-worker@*.service",
    ]);
    let output = output_with_timeout(command, COMMAND_TIMEOUT).await?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    parse_inventory(&String::from_utf8(output.stdout).context("invalid systemctl output")?)
}

pub(super) fn parse_inventory(output: &str) -> Result<BTreeMap<GenerationId, Liveness>> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_inventory_line)
        .collect()
}

fn parse_inventory_line(line: &str) -> Result<(GenerationId, Liveness)> {
    let mut fields = line.split_whitespace();
    let name = fields.next().context("worker unit row has no name")?;
    let _load = fields.next().context("worker unit row has no load state")?;
    let active = fields
        .next()
        .context("worker unit row has no active state")?;
    let generation = name
        .strip_prefix("toks-router-worker@")
        .and_then(|value| value.strip_suffix(".service"))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value != 0)
        .context("invalid worker generation unit")?;
    let liveness = match active {
        "inactive" | "failed" => Liveness::Stopped,
        "active" | "activating" | "reloading" | "deactivating" | "maintenance" => Liveness::Running,
        _ => Liveness::Unknown,
    };
    Ok((GenerationId::from_raw(generation), liveness))
}

fn unit_name(generation: &GenerationId) -> String {
    format!("toks-router-worker@{}.service", generation.get())
}

pub(super) async fn output_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<std::process::Output> {
    command.kill_on_drop(true);
    tokio::time::timeout(timeout, command.output())
        .await
        .context("worker systemd command timed out")?
        .context("running worker systemd unit")
}
