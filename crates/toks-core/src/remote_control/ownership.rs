use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use super::{RemoteConnection, RemoteConnectionStatus, RemoteControlOwner, RemoteControlSnapshot};

const DESKTOP_CLIENT_NAME: &str = "Codex Desktop";

pub(super) fn desktop_snapshot(codex_home: &Path) -> Option<RemoteControlSnapshot> {
    desktop_snapshot_at(codex_home, Path::new("/proc"), default_codex_home())
}

fn desktop_snapshot_at(
    codex_home: &Path,
    proc_root: &Path,
    default_home: Option<PathBuf>,
) -> Option<RemoteControlSnapshot> {
    desktop_app_server(proc_root, codex_home, default_home.as_deref())?;
    let account_id = current_account_id(codex_home)?;
    let (server_name, environment_id) = enabled_enrollment(codex_home, &account_id)?;
    Some(RemoteControlSnapshot {
        connection: RemoteConnection {
            status: RemoteConnectionStatus::Managed(RemoteControlOwner::ChatGptDesktop),
            server_name: Some(server_name),
        },
        environment_id: Some(environment_id),
        ..Default::default()
    })
}

fn desktop_app_server(
    proc_root: &Path,
    codex_home: &Path,
    default_home: Option<&Path>,
) -> Option<()> {
    for entry in fs::read_dir(proc_root).ok()?.flatten() {
        let process = entry.path();
        if !is_executable(&process, "ChatGPT") || has_argument_prefix(&process, "--type=") {
            continue;
        }
        let Some(uid) = process.metadata().ok().map(|metadata| metadata.uid()) else {
            continue;
        };
        for child in direct_children(&process) {
            if child
                .metadata()
                .ok()
                .is_some_and(|metadata| metadata.uid() == uid)
                && is_executable(&child, "codex")
                && has_argument(&child, "app-server")
                && uses_codex_home(&child, codex_home, default_home)
            {
                return Some(());
            }
        }
    }
    None
}

fn direct_children(process: &Path) -> Vec<PathBuf> {
    let Some(pid) = process.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    let Ok(children) = fs::read_to_string(process.join("task").join(pid).join("children")) else {
        return Vec::new();
    };
    children
        .split_whitespace()
        .map(|child| process.with_file_name(child))
        .collect()
}

fn is_executable(process: &Path, name: &str) -> bool {
    fs::read_link(process.join("exe"))
        .ok()
        .and_then(|path| path.file_name().map(|value| value == name))
        .unwrap_or(false)
}

fn has_argument(process: &Path, argument: &str) -> bool {
    fs::read(process.join("cmdline")).ok().is_some_and(|raw| {
        raw.split(|byte| *byte == 0)
            .any(|value| value == argument.as_bytes())
    })
}

fn has_argument_prefix(process: &Path, prefix: &str) -> bool {
    fs::read(process.join("cmdline")).ok().is_some_and(|raw| {
        raw.split(|byte| *byte == 0)
            .any(|value| value.starts_with(prefix.as_bytes()))
    })
}

fn uses_codex_home(process: &Path, expected: &Path, default_home: Option<&Path>) -> bool {
    let configured = fs::read(process.join("environ")).ok().and_then(|raw| {
        raw.split(|byte| *byte == 0).find_map(|entry| {
            entry
                .strip_prefix(b"CODEX_HOME=")
                .and_then(|value| std::str::from_utf8(value).ok())
                .map(PathBuf::from)
        })
    });
    configured.as_deref().or(default_home) == Some(expected)
}

fn current_account_id(codex_home: &Path) -> Option<String> {
    let auth: Value = serde_json::from_slice(&fs::read(codex_home.join("auth.json")).ok()?).ok()?;
    auth.pointer("/tokens/account_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn enabled_enrollment(codex_home: &Path, account_id: &str) -> Option<(String, String)> {
    let connection = Connection::open_with_flags(
        codex_home.join("state_5.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .query_row(
            "SELECT server_name, environment_id FROM remote_control_enrollments \
             WHERE account_id = ?1 AND app_server_client_name = ?2 \
             AND remote_control_enabled = 1 ORDER BY updated_at DESC LIMIT 1",
            (account_id, DESKTOP_CLIENT_NAME),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .ok()?
}

fn default_codex_home() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex"))
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
