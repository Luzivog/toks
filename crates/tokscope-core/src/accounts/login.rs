use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::limits::Provider;

use super::{
    discover_profiles, now_millis, now_nanos, profiles_root, restrict_directory, write_metadata,
    AddAccountStarted, ProfileMetadata, PROFILE_VERSION,
};

/// Start the provider's official login command in an isolated terminal. Only
/// non-secret profile metadata is persisted by Tokscope.
pub fn begin_add_account(provider: Provider) -> Result<AddAccountStarted> {
    let cli_name = match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    let cli = find_executable(cli_name)
        .with_context(|| format!("{cli_name} is not installed or is not on PATH"))?;
    let terminal = find_terminal().context("no supported terminal application was found")?;

    let provider_root = profiles_root()?.join(provider.slug());
    fs::create_dir_all(&provider_root)
        .with_context(|| format!("creating {} profile storage", provider.display_name()))?;
    restrict_directory(&provider_root)?;

    let created_at_ms = now_millis()?;
    let id = format!("{:x}-{:x}", now_nanos()?, std::process::id());
    let root = provider_root.join(&id);
    let home = root.join("home");
    let config = match provider {
        Provider::Claude => home.join(".claude"),
        Provider::Codex => home.join(".codex"),
    };

    fs::create_dir_all(&config).context("creating isolated provider profile")?;
    restrict_directory(&root)?;
    restrict_directory(&home)?;
    restrict_directory(&config)?;

    let metadata = ProfileMetadata {
        version: PROFILE_VERSION,
        id: id.clone(),
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
    watch_login(child, root, config, provider);

    Ok(AddAccountStarted {
        provider,
        account_id: id,
    })
}

/// Reopen the provider-owned sign-in flow for one existing local profile.
/// The profile is neither replaced nor removed, so its last-good usage and
/// stable account identity remain intact throughout reauthentication.
pub fn begin_reauthentication(provider: Provider, account_id: &str) -> Result<()> {
    let profile = exact_profile(discover_profiles(), provider, account_id)
        .context("account profile is no longer available")?;
    let cli_name = match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    let cli = find_executable(cli_name)
        .with_context(|| format!("{cli_name} is not installed or is not on PATH"))?;
    let terminal = find_terminal().context("no supported terminal application was found")?;
    let mut child = spawn_login(
        &terminal,
        &cli,
        provider,
        &profile.home_dir,
        &profile.config_dir,
    )?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub(super) fn exact_profile(
    profiles: Vec<super::AccountProfile>,
    provider: Provider,
    account_id: &str,
) -> Option<super::AccountProfile> {
    profiles
        .into_iter()
        .find(|profile| profile.provider == provider && profile.account.id == account_id)
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
        TerminalKind::Gnome => {
            command.args(["--wait", "--"]);
        }
        TerminalKind::GnomeConsole => {
            command.arg("--");
        }
        TerminalKind::Konsole | TerminalKind::XTerminal => {
            command.arg("-e");
        }
    }
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

fn watch_login(
    mut child: std::process::Child,
    profile_root: PathBuf,
    config: PathBuf,
    provider: Provider,
) {
    std::thread::spawn(move || {
        let _ = child.wait();
        // Some terminal launchers detach from their command. Give the provider
        // CLI time to finish writing credentials before treating the profile as
        // an abandoned sign-in.
        for _ in 0..300 {
            if credentials_file(provider, &config).is_file() {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        let _ = fs::remove_dir_all(profile_root);
    });
}

fn credentials_file(provider: Provider, config: &Path) -> PathBuf {
    match provider {
        Provider::Claude => config.join(".credentials.json"),
        Provider::Codex => config.join("auth.json"),
    }
}
