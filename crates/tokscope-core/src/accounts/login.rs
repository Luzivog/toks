use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::limits::Provider;

use super::{
    discover_profiles, now_millis, now_nanos, profiles_root, restrict_directory, write_metadata,
    AddAccountStarted, CredentialProfileId, ProfileMetadata, PROFILE_VERSION,
};

mod lifecycle;
pub use lifecycle::{cancel_login, login_outcome, LoginOutcome};

/// Start the provider's official login command in an isolated terminal. Only
/// non-secret profile metadata is persisted by Tokscope.
pub fn begin_add_account(provider: Provider) -> Result<AddAccountStarted> {
    let (terminal, cli) = login_command(provider)?;
    let provider_root = profiles_root()?.join(provider.slug());
    fs::create_dir_all(&provider_root)
        .with_context(|| format!("creating {} profile storage", provider.display_name()))?;
    restrict_directory(&provider_root)?;

    let created_at_ms = now_millis()?;
    let id = format!("{:x}-{:x}", now_nanos()?, std::process::id());
    let profile_id = CredentialProfileId::new(id.clone());
    let root = provider_root.join(&id);
    let home = root.join("home");
    let config = provider_config(provider, &home);
    fs::create_dir_all(&config).context("creating isolated provider profile")?;
    for directory in [&root, &home, &config] {
        restrict_directory(directory)?;
    }
    let metadata = ProfileMetadata {
        version: PROFILE_VERSION,
        id,
        provider,
        created_at_ms,
    };
    if let Err(error) = write_metadata(&root.join("profile.json"), &metadata) {
        let _ = fs::remove_dir_all(&root);
        return Err(error);
    }
    let child = match spawn_login(&terminal, &cli, provider, &home, &config) {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
    };
    lifecycle::track_add(child, provider, profile_id.clone(), config);
    Ok(AddAccountStarted {
        provider,
        account_id: profile_id,
    })
}

/// Reopen sign-in for one exact credential profile. Completion is reported by
/// [`login_outcome`], including a typed identity transition when reauth signs
/// the profile into a different provider account.
pub fn begin_reauthentication(provider: Provider, profile_id: &CredentialProfileId) -> Result<()> {
    let profile = exact_profile(discover_profiles(), provider, profile_id)
        .context("account profile is no longer available")?;
    let (terminal, cli) = login_command(provider)?;
    let before = super::provider_principal_id(&profile);
    let before_stamp = lifecycle::credential_stamp(provider, &profile.config_dir);
    let child = spawn_login(
        &terminal,
        &cli,
        provider,
        &profile.home_dir,
        &profile.config_dir,
    )?;
    lifecycle::track_reauthentication(child, profile, before, before_stamp);
    Ok(())
}

pub(super) fn exact_profile(
    profiles: Vec<super::AccountProfile>,
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> Option<super::AccountProfile> {
    profiles
        .into_iter()
        .find(|profile| profile.provider == provider && profile.profile_id == *profile_id)
}

fn login_command(provider: Provider) -> Result<((PathBuf, TerminalKind), PathBuf)> {
    let cli_name = match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    let cli = find_executable(cli_name)
        .with_context(|| format!("{cli_name} is not installed or is not on PATH"))?;
    let terminal = find_terminal().context("no supported terminal application was found")?;
    Ok((terminal, cli))
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn find_terminal() -> Option<(PathBuf, TerminalKind)> {
    [
        ("gnome-terminal", TerminalKind::Gnome),
        ("kgx", TerminalKind::GnomeConsole),
        ("konsole", TerminalKind::Konsole),
        ("x-terminal-emulator", TerminalKind::XTerminal),
    ]
    .into_iter()
    .find_map(|(name, kind)| find_executable(name).map(|path| (path, kind)))
}

#[derive(Debug, Clone, Copy)]
enum TerminalKind {
    Gnome,
    GnomeConsole,
    Konsole,
    XTerminal,
}

fn spawn_login(
    terminal: &(PathBuf, TerminalKind),
    cli: &Path,
    provider: Provider,
    home: &Path,
    config: &Path,
) -> Result<std::process::Child> {
    let mut command = Command::new(&terminal.0);
    match terminal.1 {
        TerminalKind::Gnome => command.args(["--wait", "--"]),
        TerminalKind::GnomeConsole => command.arg("--"),
        TerminalKind::Konsole | TerminalKind::XTerminal => command.arg("-e"),
    };
    command.arg(cli);
    match provider {
        Provider::Claude => {
            command.args(["auth", "login", "--claudeai"]);
            command.env("CLAUDE_CONFIG_DIR", config);
        }
        Provider::Codex => {
            command.arg("login");
            command.env("CODEX_HOME", config);
        }
    }
    command
        .env("HOME", home)
        .current_dir(home)
        .spawn()
        .context("opening the provider sign-in terminal")
}

fn provider_config(provider: Provider, home: &Path) -> PathBuf {
    match provider {
        Provider::Claude => home.join(".claude"),
        Provider::Codex => home.join(".codex"),
    }
}

#[cfg(test)]
#[path = "login/tests.rs"]
mod tests;
