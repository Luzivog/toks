use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub(crate) fn discover() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("TOKS_CODEX_BIN").map(PathBuf::from) {
        return validate(path);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join("codex");
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        for candidate in [home.join(".local/bin/codex"), home.join(".cargo/bin/codex")] {
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    bail!("Codex CLI was not found; install it before enabling rotation")
}

fn validate(path: PathBuf) -> Result<PathBuf> {
    if is_executable(&path) {
        Ok(path)
    } else {
        Err(anyhow::anyhow!(
            "Codex CLI is not executable at {}",
            path.display()
        ))
        .context("invalid TOKS_CODEX_BIN")
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    metadata.is_file()
}
