use std::path::Path;

use anyhow::{Context, Result};

use crate::codex_router::host::BuildId;

use super::units::{environment, quote, UnitEnvironment};

pub(super) fn render(
    executable: &Path,
    codex_executable: &Path,
    build: &BuildId,
    unit_environment: &UnitEnvironment,
) -> Result<String> {
    let executable = quote(executable)?;
    let codex = codex_executable
        .to_str()
        .context("Codex executable path is not UTF-8")?;
    let codex_environment = environment("TOKS_CODEX_BIN", codex)?;
    let build_environment = environment("TOKS_ROUTER_BUILD_ID", build.as_str())?;
    let inherited_environment = unit_environment.directives()?;
    Ok(format!(
        "[Unit]\nDescription=Toks waiting-task resume supervisor\nAfter=network-online.target toks-router.service\nWants=network-online.target toks-router.service\n\n[Service]\nType=simple\nExecStart={executable} launch-resume-supervisor\nEnvironment={codex_environment}\nEnvironment={build_environment}{inherited_environment}\nRestart=always\nRestartSec=2\nKillMode=control-group\nTimeoutStopSec=15s\nUMask=0077\nNoNewPrivileges=true\nProtectSystem=full\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nLockPersonality=true\n\n[Install]\nWantedBy=default.target\n"
    ))
}
